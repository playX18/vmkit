use std::ops::Range;

pub struct MmapStack {
    pub(crate) mapping_base: *mut u8,
    pub(crate) mapping_len: usize,
}

unsafe impl Send for MmapStack {}
unsafe impl Sync for MmapStack {}

impl MmapStack {
    #[cfg(unix)]
    pub fn new(size: usize) -> std::io::Result<Self> {
        // Round up our stack size request to the nearest multiple of the
        // page size.
        let page_size = rustix::param::page_size();
        let size = if size == 0 {
            page_size
        } else {
            (size + (page_size - 1)) & (!(page_size - 1))
        };

        unsafe {
            // Add in one page for a guard page and then ask for some memory.
            let mmap_len = size + page_size;
            let mmap = rustix::mm::mmap_anonymous(
                std::ptr::null_mut(),
                mmap_len,
                rustix::mm::ProtFlags::empty(),
                rustix::mm::MapFlags::PRIVATE | rustix::mm::MapFlags::GROWSDOWN,
            )?;

            rustix::mm::mprotect(
                mmap.byte_add(page_size),
                size,
                rustix::mm::MprotectFlags::READ | rustix::mm::MprotectFlags::WRITE,
            )?;

            Ok(MmapStack {
                mapping_base: mmap.cast(),
                mapping_len: mmap_len,
            })
        }
    }

    #[cfg(windows)]
    pub fn new(size: usize) -> std::io::Result<Self> {
        use winapi::um::memoryapi::*;
        use winapi::um::sysinfoapi::*;
        use winapi::um::winnt::*;
        let mut sys_info: std::mem::MaybeUninit<SYSTEM_INFO> = std::mem::MaybeUninit::uninit();
        unsafe {
            GetSystemInfo(sys_info.as_mut_ptr());
            let page_size = sys_info.as_ptr().as_ref().unwrap().page_size as usize;

            let size = if size == 0 {
                page_size
            } else {
                (size + (page_size - 1)) & (!(page_size - 1))
            };

            let mmap_len = size + page_size;

            let mmap = VirtualAlloc(
                std::ptr::null_mut(),
                mmap_len,
                MAP_COMMIT | MAP_RESERVE,
                PAGE_GUARD,
            );
            VirtualProtect(mmap, size, PAGE_READWRITE, std::ptr::null_mut());

            Self {
                mapping_base: mmap as _,
                mapping_len: mmap_len,
            }
        }
    }
}

impl Drop for MmapStack {
    #[cfg(unix)]
    fn drop(&mut self) {
        unsafe {
            let ret = rustix::mm::munmap(self.mapping_base.cast(), self.mapping_len);
            debug_assert!(ret.is_ok());
        }
    }

    #[cfg(windows)]
    fn drop(&mut self) {
        unsafe {
            use winapi::um::memoryapi::*;
            use winapi::um::winnt::*;

            VirtualFree(self.mapping_base as _, self.mapping_len as _, MEM_RELEASE);
        }
    }
}

pub enum StackStorage {
    Mmap(MmapStack),
    Unmanaged(*mut u8, usize),
    Custom(Box<dyn StackContext>),
}

impl StackStorage {
    pub fn size(&self) -> usize {
        match self {
            Self::Mmap(mmap) => mmap.mapping_len - rustix::param::page_size(),
            Self::Unmanaged(_, size) => *size,
            Self::Custom(custom) => custom.size(),
        }
    }

    pub fn top(&self) -> *mut u8 {
        match self {
            Self::Mmap(mmap) => unsafe {
                mmap.mapping_base
                    .byte_add(mmap.mapping_len - rustix::param::page_size())
            },

            Self::Unmanaged(base, size) => unsafe { base.byte_add(*size) },
            Self::Custom(custom) => custom.top(),
        }
    }
}

pub trait StackContext {
    fn top(&self) -> *mut u8;
    fn size(&self) -> usize;
    fn range(&self) -> Range<usize>;
    fn guard_range(&self) -> Range<*mut u8>;
}
