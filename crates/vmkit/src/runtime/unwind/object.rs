use std::{ops::Range, path::PathBuf};

cfg_if::cfg_if! {
    if #[cfg(any(target_os="linux", target_os="freebsd"))] {
        mod impl_ {
            use std::{
                env,
                ffi::{c_int, c_void, CStr, OsString},
                fs::File,
                mem::ManuallyDrop,
                os::unix::ffi::OsStringExt,
                path::{Path, PathBuf},
                sync::LazyLock,
            };

            use libc::{dl_iterate_phdr, dl_phdr_info, size_t, PF_R, PF_X, PT_LOAD};
            use log::warn;
            use memmap2::Mmap;

            use super::{Object, ObjectPHdr, Segment};
            pub struct ObjectMmap {
                pub file: ManuallyDrop<File>,
                pub mmap: ManuallyDrop<Mmap>,
                pub obj_file: ManuallyDrop<object::File<'static, &'static [u8]>>,
            }

            impl ObjectMmap {
                fn new(path: &Path) -> Option<ObjectMmap> {
                    let file = File::open(path)
                        .map_err(|e| warn!("Failed to open {path:?}: {e}"))
                        .ok()?;
                    let mmap = unsafe {
                        Mmap::map(&file)
                            .map_err(|e| warn!("Failed to mmap {path:?}: {e}"))
                            .ok()?
                    };
                    let (ptr, len) = (mmap.as_ptr(), mmap.len());
                    let data = unsafe { std::slice::from_raw_parts(ptr, len) };
                    let obj_file = object::File::parse(data)
                        .map_err(|e| warn!("Failed to parse {path:?}: {e}"))
                        .ok()?;
                    Some(ObjectMmap {
                        file: ManuallyDrop::new(file),
                        mmap: ManuallyDrop::new(mmap),
                        obj_file: ManuallyDrop::new(obj_file),
                    })
                }
            }

            impl Drop for ObjectMmap {
                fn drop(&mut self) {
                    // Specify drop order:
                    // 1. Drop the object::File that may reference the mmap.
                    // 2. Drop the mmap.
                    // 3. Close the file.
                    unsafe {
                        ManuallyDrop::drop(&mut self.obj_file);
                        ManuallyDrop::drop(&mut self.mmap);
                        ManuallyDrop::drop(&mut self.file);
                    };
                }
            }

            static OBJECTS: LazyLock<Vec<Object>> = LazyLock::new(find_objects);

            pub fn get_objects() -> &'static [Object] {
                &OBJECTS
            }

            fn find_objects() -> Vec<Object> {
                let mut objects = Vec::new();
                unsafe {
                    dl_iterate_phdr(
                        Some(iterate_phdr_cb),
                        &mut objects as *mut Vec<Object> as *mut c_void,
                    );
                }
                objects
            }

            unsafe extern "C" fn iterate_phdr_cb(
                info: *mut dl_phdr_info,
                _size: size_t,
                data: *mut c_void,
            ) -> c_int {
                let info = &*info;
                let base_addr = info.dlpi_addr as usize;

                // The dlpi_name of the current executable is a empty C string.
                let path = if *info.dlpi_name == 0 {
                    match env::current_exe() {
                        Ok(path) => path,
                        Err(e) => {
                            warn!("Could not get current executable path: {e}");
                            return 0;
                        }
                    }
                } else {
                    PathBuf::from(OsString::from_vec(
                        CStr::from_ptr(info.dlpi_name).to_bytes().to_vec(),
                    ))
                };
                let mut text = None;

                let phdrs = std::slice::from_raw_parts(info.dlpi_phdr, info.dlpi_phnum as usize);
                for phdr in phdrs {
                    let segment = Segment {
                        p_vaddr: phdr.p_vaddr as usize,
                        p_memsz: phdr.p_memsz as usize,
                    };
                    match phdr.p_type {
                        // .text segment
                        PT_LOAD if phdr.p_flags == PF_X | PF_R => {
                            if text.is_some() {
                                warn!("Multiple text segments found in {path:?}");
                            }
                            text = Some(segment);
                        }
                        // Ignore other segments
                        _ => {}
                    }
                }

                let text = match text {
                    Some(text) => text,
                    None => {
                        warn!("No text segment found in {path:?}");
                        return 0;
                    }
                };

                let phdr = ObjectPHdr {
                    base_addr,
                    path,
                    text,
                };
                if let Some(mmap) = ObjectMmap::new(&phdr.path) {
                    let objects = &mut *(data as *mut Vec<Object>);
                    objects.push(Object { phdr, mmap });
                }

                0
            }
        }
    } else if #[cfg(target_os="macos")] {
        mod impl {
            static OBJECTS: LazyLock<Vec<Object>> = LazyLock::new(find_objects);

            pub fn get_objects() -> &'static [Object] {
                &OBJECTS
            }

            fn find_objects() -> Vec<Object> {
                vec![]
            }
        }
    } else {
        mod impl_ {
            static OBJECTS: LazyLock<Vec<Object>> = LazyLock::new(find_objects);

            pub fn get_objects() -> &'static [Object] {
                &OBJECTS
            }

            fn find_objects() -> Vec<Object> {
                vec![]
            }
        }
    }
}

use framehop::Module;
use framehop_object::ObjectSectionInfo;
use impl_::*;

pub use impl_::get_objects;

#[derive(Debug)]
pub struct ObjectPHdr {
    base_addr: usize,
    path: PathBuf,
    text: Segment,
}
#[derive(Debug)]
pub struct Segment {
    p_vaddr: usize,
    p_memsz: usize,
}

pub struct Object {
    phdr: ObjectPHdr,
    mmap: ObjectMmap,
}

impl Object {
    pub fn to_module(&self) -> Module<&'_ [u8]> {
        let name = self.phdr.path.to_string_lossy().to_string();
        let base_avma = self.phdr.base_addr as u64;
        let text_range = (self.phdr.base_addr + self.phdr.text.p_vaddr) as u64
            ..(self.phdr.base_addr + self.phdr.text.p_vaddr + self.phdr.text.p_memsz) as u64;

        Module::new(
            name,
            text_range,
            base_avma,
            ObjectSectionInfo::from_ref(self.obj_file()),
        )
    }

    pub fn obj_file(&self) -> &'_ object::File<'static, &'static [u8]> {
        &*self.mmap.obj_file
    }

    pub fn base_addr(&self) -> usize {
        self.phdr.base_addr
    }

    pub fn text_svma(&self) -> Range<usize> {
        self.phdr.text.p_vaddr..(self.phdr.text.p_vaddr + self.phdr.text.p_memsz)
    }
}

impl std::fmt::Debug for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Object").field("phdr", &self.phdr).finish()
    }
}
