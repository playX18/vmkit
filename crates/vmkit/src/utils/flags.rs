//! A small library to parse command-line and environmental flags.

use std::{
    any::TypeId, borrow::Cow, cell::UnsafeCell, marker::PhantomData, ptr::null_mut,
    sync::atomic::AtomicBool,
};

use parking_lot::Mutex;

use crate::utils::parse_float_and_factor_from_str;

use super::MemorySize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FlagType {
    Boolean,
    Isize,
    Usize,
    F64,
    MemorySize,
    String,
    FlagHandler,
    OptionHandler,
}

pub type FlagHandler = fn(bool);
pub type OptionHandler = fn(&str);

struct Flag {
    #[allow(dead_code)]
    comment: &'static str,
    name: &'static str,
    is_set: AtomicBool,
    short: Option<&'static str>,

    string_value: Option<String>,
    u: FlagValue,
    typ: FlagType,
    changed: bool,
}

#[repr(C)]
union FlagValue {
    addr: *mut u8,
    bool_ptr: *mut bool,
    int_ptr: *mut isize,
    u64_ptr: *mut usize,
    f64_ptr: *mut f64,
    msize_ptr: *mut MemorySize,
    char_ptr: *mut u8,
    flag_handler: FlagHandler,
    option_handler: OptionHandler,
}

impl Flag {
    fn new_type(
        name: &'static str,
        comment: &'static str,
        addr: *mut u8,
        typ: FlagType,
        short: Option<&'static str>,
    ) -> Self {
        Self {
            comment,
            name,
            string_value: None,
            u: FlagValue { addr },
            typ,
            is_set: AtomicBool::new(false),
            changed: false,
            short,
        }
    }

    fn new_handler(
        name: &'static str,
        comment: &'static str,
        handler: FlagHandler,
        short: Option<&'static str>,
    ) -> Self {
        Self {
            comment,
            name,
            string_value: None,
            u: FlagValue {
                flag_handler: handler,
            },
            is_set: AtomicBool::new(false),
            typ: FlagType::FlagHandler,
            changed: false,
            short,
        }
    }

    fn new_option(
        name: &'static str,
        comment: &'static str,
        handler: OptionHandler,
        short: Option<&'static str>,
    ) -> Self {
        Self {
            comment,
            name,
            string_value: None,
            u: FlagValue {
                option_handler: handler,
            },
            typ: FlagType::OptionHandler,
            is_set: AtomicBool::new(false),
            changed: false,
            short,
        }
    }

    fn is_unrecognized(&self) -> bool {
        self.typ == FlagType::Boolean && unsafe { self.u.bool_ptr.is_null() }
    }
}

pub struct Flags {
    flags: *mut *mut Flag,
    capacity: usize,
    len: usize,
    initialized: bool,
}

/// A map of type-id -> flags
///
/// This is essential because we have to build this table before Rust std is initialized,
/// so we use libc for this map.
struct FlagsMap {
    nodes: *mut Node,
    length: usize,
    capacity: usize,
}

struct Node {
    type_id: TypeId,
    flags: Mutex<Flags>,
}

impl FlagsMap {
    const fn new() -> Self {
        Self {
            nodes: null_mut(),
            capacity: 0,
            length: 0,
        }
    }

    unsafe fn nodes<'a>(&self) -> &'a [Node] {
        std::slice::from_raw_parts(self.nodes, self.length)
    }

    unsafe fn init(&mut self) {
        self.nodes = libc::calloc(8, size_of::<Node>()).cast();
        self.capacity = 8;
    }

    unsafe fn get_or_insert(&mut self, key: TypeId) -> &'static Mutex<Flags> {
        if self.nodes.is_null() {
            self.init();
        }
        let nodes = self.nodes();
        for node in nodes {
            if node.type_id == key {
                return &node.flags;
            }
        }

        let ix = self.length;

        if ix >= self.capacity {
            self.resize();
        }

        self.nodes.add(ix).write(Node {
            flags: Mutex::new(Flags {
                flags: null_mut(),
                capacity: 0,
                len: 0,
                initialized: false,
            }),
            type_id: key,
        });
        self.length += 1;

        &self.nodes.add(ix).as_ref().unwrap().flags
    }

    unsafe fn try_get(&self, key: TypeId) -> Option<&'static Mutex<Flags>> {
        let nodes = self.nodes();
        for node in nodes {
            if node.type_id == key {
                return Some(&node.flags);
            }
        }

        None
    }

    unsafe fn resize(&mut self) {
        let size = self.capacity * 2;

        let new_nodes = libc::calloc(size, size_of::<Node>()).cast::<Node>();
        self.capacity = size;
        new_nodes.copy_from_nonoverlapping(self.nodes, self.length);
        libc::free(self.nodes.cast());
        self.nodes = new_nodes;
    }
}

pub struct FlagsOf<T>(PhantomData<T>);

unsafe impl Send for FlagsMap {}
unsafe impl Sync for FlagsMap {}

struct MapInner(UnsafeCell<FlagsMap>);

unsafe impl Send for MapInner {}
unsafe impl Sync for MapInner {}

static FLAGS_MAP: MapInner = MapInner(UnsafeCell::new(FlagsMap::new()));

impl<T: 'static> FlagsOf<T> {
    pub fn get() -> &'static Mutex<Flags> {
        unsafe {
            FLAGS_MAP
                .0
                .get()
                .as_mut()
                .unwrap()
                .get_or_insert(TypeId::of::<T>())
        }
    }

    fn lookup(name: &str) -> Option<&'static mut Flag> {
        let flags = Self::get().lock();

        for i in 0..flags.len {
            let flag = unsafe { &mut **flags.flags.add(i) };
            if flag.name == name {
                return Some(flag);
            }
        }

        None
    }

    fn lookup_short(short: &str) -> Option<&'static mut Flag> {
        let flags = Self::get().lock();

        for i in 0..flags.len {
            let flag = unsafe { &mut **flags.flags.add(i) };
            if flag.short == Some(short) {
                return Some(flag);
            }
        }

        None
    }

    pub fn is_set(name: &str) -> bool {
        let flag = Self::lookup(name);

        flag.map_or(false, |flag| {
            flag.is_set.load(std::sync::atomic::Ordering::Relaxed)
        })
    }

    unsafe fn add_flag(flag: *mut Flag) {
        let mut flags = Self::get().lock();

        if flags.len == flags.capacity {
            if flags.flags.is_null() {
                flags.capacity = 256;
                flags.flags = libc::calloc(flags.capacity, std::mem::size_of::<*mut Flag>())
                    as *mut *mut Flag;
            } else {
                let new_capacity = flags.capacity * 2;
                let new_flags = libc::realloc(
                    flags.flags as *mut libc::c_void,
                    new_capacity * std::mem::size_of::<*mut Flag>(),
                ) as *mut *mut Flag;

                flags.capacity = new_capacity;

                for i in 0..flags.len {
                    new_flags.add(i).write(flags.flags.add(i).read());
                }
                flags.flags = new_flags;
            }
        }

        flags.flags.add(flags.len).write(flag);
        flags.len += 1;
    }

    fn set_flag_from_string(flag: &mut Flag, argument: &str) -> bool {
        assert!(!flag.is_unrecognized());

        match flag.typ {
            FlagType::Boolean => {
                if argument == "true" {
                    unsafe {
                        *flag.u.bool_ptr = true;
                    }
                } else if argument == "false" {
                    unsafe {
                        *flag.u.bool_ptr = false;
                    }
                } else {
                    return false;
                }
            }

            FlagType::String => {
                flag.string_value = Some(argument.to_owned());
            }

            FlagType::Isize => {
                let len = argument.len();

                let mut base = 10;

                if len > 2 && &argument[0..2] == "0x" {
                    base = 16;
                } else if len > 1 && &argument[0..1] == "0" {
                    base = 8;
                }

                let value = isize::from_str_radix(argument, base);

                match value {
                    Ok(value) => unsafe {
                        *flag.u.int_ptr = value;
                    },
                    Err(_) => return false,
                }
            }

            FlagType::Usize => {
                let len = argument.len();

                let mut base = 10;

                if len > 2 && &argument[0..2] == "0x" {
                    base = 16;
                } else if len > 1 && &argument[0..1] == "0" {
                    base = 8;
                }

                let value = usize::from_str_radix(argument, base);

                match value {
                    Ok(value) => unsafe {
                        *flag.u.u64_ptr = value;
                    },
                    Err(_) => return false,
                }
            }

            FlagType::FlagHandler => {
                if argument == "true" {
                    unsafe {
                        (flag.u.flag_handler)(true);
                    }
                } else if argument == "false" {
                    unsafe {
                        (flag.u.flag_handler)(false);
                    }
                } else {
                    return false;
                }
            }

            FlagType::OptionHandler => {
                flag.string_value = Some(argument.to_owned());
                unsafe {
                    (flag.u.option_handler)(argument);
                }
            }

            FlagType::MemorySize => {
                let val = parse_float_and_factor_from_str(argument);
                if let Some((float, factor)) = val {
                    unsafe {
                        *flag.u.msize_ptr = MemorySize((float * factor as f64) as usize);
                    }
                } else {
                    return false;
                }
            }

            FlagType::F64 => {
                let val = argument.parse::<f64>();

                match val {
                    Ok(val) => unsafe {
                        *flag.u.f64_ptr = val;
                    },
                    Err(_) => return false,
                }
            }
        }
        flag.is_set
            .store(true, std::sync::atomic::Ordering::Relaxed);
        flag.changed = true;

        true
    }

    fn parse<const SHORT: bool>(option: &str) -> Result<(), FlagError> {
        let equals_pos = option.find('=');

        let argument;

        if let Some(equals_pos) = equals_pos {
            argument = &option[equals_pos + 1..];
        } else {
            const NO_1_PREFIX: &'static str = "no_";
            const NO_2_PREFIX: &'static str = "no-";

            if option.len() > NO_1_PREFIX.len() && &option[0..NO_1_PREFIX.len()] == NO_1_PREFIX {
                argument = "false";
            } else if option.len() > NO_2_PREFIX.len()
                && &option[0..NO_2_PREFIX.len()] == NO_2_PREFIX
            {
                argument = "false";
            } else {
                argument = "true";
            }
        }

        let name_len = if let Some(equals_pos) = equals_pos {
            equals_pos
        } else {
            option.len()
        };
        let name = option[0..name_len].replace('-', "_");

        let Some(flag) = (if !SHORT {
            Self::lookup(&name)
        } else {
            Self::lookup_short(&name)
        }) else {
            return Err(FlagError::FlagNotFound(name.to_owned()));
        };

        if !flag.is_unrecognized() {
            if !Self::set_flag_from_string(flag, argument) {
                eprintln!(
                    "Ignoring flag: {} is an invalid value for flag {}",
                    argument, name
                );
            }
        }

        Ok(())
    }

    fn parse_env(option: &str, argument: &str) -> Result<(), FlagError> {
        let name = option.to_lowercase();
        if let Some(flag) = Self::lookup(&name) {
            if !flag.is_unrecognized() {
                if !Self::set_flag_from_string(flag, argument) {
                    eprintln!(
                        "Ignoring flag: {} is an invalid value for flag {}",
                        argument, name
                    );
                }
            }
            Ok(())
        } else {
            Err(FlagError::FlagNotFound(option.to_owned()))
        }
    }

    fn process_command_line_flags(
        prefix: Option<&str>,
        flags: impl Iterator<Item = String>,
    ) -> Result<(), FlagError> {
        let mut flags_vec = flags.collect::<Vec<String>>();
        flags_vec.sort_by(|a, b| compare_flag_names(a, b));

        let cli_prefix = prefix
            .map(|prefix| Cow::Owned(format!("--{}:", prefix)))
            .unwrap_or(Cow::Borrowed("--"));

        let mut i = 0;

        while i < flags_vec.len() {
            if is_valid_flag(&flags_vec[i], &cli_prefix) {
                let option = &flags_vec[i][cli_prefix.len()..];

                Self::parse::<false>(option)?;
            }
            i += 1;
        }

        let cli_prefix = prefix
            .map(|prefix| Cow::Owned(format!("-{}:", prefix)))
            .unwrap_or(Cow::Borrowed("-"));

        while i < flags_vec.len() {
            if is_valid_flag(&flags_vec[i], &cli_prefix) {
                let option = &flags_vec[i][cli_prefix.len()..];
                Self::parse::<true>(option)?;
            }
            i += 1;
        }

        Self::get().lock().initialized = true;
        Ok(())
    }

    fn process_environmental_vars(
        prefix: Option<&str>,
        vars: impl Iterator<Item = (String, String)>,
    ) {
        let env_prefix = prefix
            .map(|prefix| format!("{}_", prefix.to_uppercase()))
            .unwrap_or(String::new());

        for (option, argument) in vars {
            match Self::parse_env(&format!("{}{}", env_prefix, option), &argument) {
                Ok(()) => (),
                Err(_) => (),
            }
        }
    }
}

fn is_valid_flag(name: &str, prefix: &str) -> bool {
    name.len() > prefix.len() && &name[0..prefix.len()] == prefix
}

fn compare_flag_names(left: &str, right: &str) -> std::cmp::Ordering {
    left.cmp(right)
}

pub fn parse<T: 'static>(
    args: impl Iterator<Item = String>,
    env: impl Iterator<Item = (String, String)>,
) -> Result<(), FlagError> {
    if let Some(flags) = unsafe {
        FLAGS_MAP
            .0
            .get()
            .as_mut()
            .unwrap()
            .try_get(TypeId::of::<T>())
    } {
        {
            if flags.lock().initialized {
                return Err(FlagError::FlagsAlreadyInitialized(
                    std::any::type_name::<T>(),
                ));
            }
        }

        FlagsOf::<T>::process_environmental_vars(None, env);
        FlagsOf::<T>::process_command_line_flags(None, args)
    } else {
        Err(FlagError::NoFlags(std::any::type_name::<T>()))
    }
}

pub fn parse_with_prefix<T: 'static>(
    prefix: &str,
    args: impl Iterator<Item = String>,
    env: impl Iterator<Item = (String, String)>,
) -> Result<(), FlagError> {
    if let Some(flags) = unsafe {
        FLAGS_MAP
            .0
            .get()
            .as_mut()
            .unwrap()
            .try_get(TypeId::of::<T>())
    } {
        if flags.lock().initialized {
            return Err(FlagError::FlagsAlreadyInitialized(
                std::any::type_name::<T>(),
            ));
        }
        FlagsOf::<T>::process_environmental_vars(Some(prefix), env);
        FlagsOf::<T>::process_command_line_flags(Some(prefix), args)
    } else {
        Err(FlagError::NoFlags(std::any::type_name::<T>()))
    }
}

/// Registers a bool flag.
///
/// # Safety
///
/// `addr` must be valid for program lifetime.
#[doc(hidden)]
pub unsafe fn register_bool<T: 'static>(
    addr: *mut bool,
    name: &'static str,
    default_value: bool,
    comment: &'static str,
    short: Option<&'static str>,
) -> bool {
    let flag = FlagsOf::<T>::lookup(name);

    if flag.is_none() {
        let flag = Flag::new_type(name, comment, addr as *mut u8, FlagType::Boolean, short);
        FlagsOf::<T>::add_flag(Box::into_raw(Box::new(flag)));
        default_value
    } else {
        default_value
    }
}

/// Registers a usize flag.
///
/// # Safety
///
/// `addr` must be valid for program lifetime.
#[doc(hidden)]
pub unsafe fn register_usize<T: 'static>(
    addr: *mut usize,
    name: &'static str,
    default_value: usize,
    comment: &'static str,
    short: Option<&'static str>,
) -> usize {
    let flag = FlagsOf::<T>::lookup(name);

    if flag.is_none() {
        let flag = Flag::new_type(name, comment, addr as *mut u8, FlagType::Usize, short);
        FlagsOf::<T>::add_flag(Box::into_raw(Box::new(flag)));
        default_value
    } else {
        default_value
    }
}

/// Registers a isize flag.
///
/// # Safety
///
/// `addr` must be valid for program lifetime.
#[doc(hidden)]
pub unsafe fn register_isize<T: 'static>(
    addr: *mut isize,
    name: &'static str,
    default_value: isize,
    comment: &'static str,
    short: Option<&'static str>,
) -> isize {
    let flag = FlagsOf::<T>::lookup(name);

    if flag.is_none() {
        let flag = Flag::new_type(name, comment, addr as *mut u8, FlagType::Isize, short);
        FlagsOf::<T>::add_flag(Box::into_raw(Box::new(flag)));
        default_value
    } else {
        default_value
    }
}

/// Register a string flag.
///
/// # Safety
///
/// `addr` must be valid for program lifetime.
#[doc(hidden)]
pub unsafe fn register_string<T: 'static>(
    addr: *mut String,
    name: &'static str,
    default_value: String,
    comment: &'static str,
    short: Option<&'static str>,
) -> String {
    let flag = FlagsOf::<T>::lookup(name);

    if flag.is_none() {
        let flag = Flag::new_type(name, comment, addr as *mut u8, FlagType::String, short);
        FlagsOf::<T>::add_flag(Box::into_raw(Box::new(flag)));
        default_value
    } else {
        default_value
    }
}

#[doc(hidden)]
pub unsafe fn register_memorysize<T: 'static>(
    addr: *mut MemorySize,
    name: &'static str,
    default_value: MemorySize,
    comment: &'static str,
    short: Option<&'static str>,
) -> MemorySize {
    let flag = FlagsOf::<T>::lookup(name);

    if flag.is_none() {
        let flag = Flag::new_type(name, comment, addr as *mut u8, FlagType::MemorySize, short);
        FlagsOf::<T>::add_flag(Box::into_raw(Box::new(flag)));
        default_value
    } else {
        default_value
    }
}

/// Registers an option handler.

#[doc(hidden)]
pub fn register_handler<T: 'static>(
    handler: OptionHandler,
    name: &'static str,
    comment: &'static str,
    short: Option<&'static str>,
) {
    let flag = FlagsOf::<T>::lookup(name);

    if flag.is_none() {
        let flag = Flag::new_option(name, comment, handler, short);
        unsafe {
            FlagsOf::<T>::add_flag(Box::into_raw(Box::new(flag)));
        }
    }
}

/// Registers an flag handler.
#[doc(hidden)]
pub fn register_flag_handler<T: 'static>(
    handler: FlagHandler,
    name: &'static str,
    comment: &'static str,
    short: Option<&'static str>,
) {
    let flag = FlagsOf::<T>::lookup(name);

    if flag.is_none() {
        let flag = Flag::new_handler(name, comment, handler, short);
        unsafe {
            FlagsOf::<T>::add_flag(Box::into_raw(Box::new(flag)));
        }
    }
}

#[doc(hidden)]
pub unsafe fn register_f64<T: 'static>(
    addr: *mut f64,
    name: &'static str,
    default_value: f64,
    comment: &'static str,
    short: Option<&'static str>,
) -> f64 {
    let flag = FlagsOf::<T>::lookup(name);

    if flag.is_none() {
        let flag = Flag::new_type(name, comment, addr as *mut u8, FlagType::F64, short);
        FlagsOf::<T>::add_flag(Box::into_raw(Box::new(flag)));
        default_value
    } else {
        default_value
    }
}

#[doc(hidden)]
pub use ctor::ctor;
#[doc(hidden)]
pub use paste;

#[macro_export]
macro_rules! define_flag {
    ($of: ident => $typ: ident, $name: ident, $default_value: expr, $comment: literal) => {
        paste::paste! {

            static mut [<$of: upper _ FLAG_ $name:upper>]: std::mem::MaybeUninit<$typ> = std::mem::MaybeUninit::uninit();

            #[doc(hidden)]
            #[ctor::ctor]
            fn [<init_ $of:lower _ $name _flag>]() {

                unsafe {
                    [<$of: upper _ FLAG_ $name:upper>].as_mut_ptr().write($default_value);
                    $crate::utils::flags::[<register_ $typ:lower>]::<$of>(
                        [<$of: upper _ FLAG_ $name:upper>].as_mut_ptr().cast(),
                        stringify!($name),
                        $default_value,
                        $comment,
                        None,
                    );
                }
            }

            pub fn [<$of: lower _ $name>]() -> &'static $typ {
                unsafe { [<$of: upper _ FLAG_ $name:upper>].assume_init_ref() }
            }

            pub fn [<set_ $of: lower _ $name>]($name: $typ) {
                unsafe {
                    *[<$of: upper _ FLAG_ $name:upper>].as_mut_ptr() = $name;
                }
            }

            pub fn [<is_ $of: lower _ $name _set>]() -> bool {
                $crate::utils::flags::FlagsOf::<$of>::is_set(stringify!($name))
            }
        }
    };

    ($of:path => $typ: ident, $name: ident, $short: literal, $default_value: expr, $comment: literal) => {
        paste::paste! {

            static mut [<$of_ FLAG_ $name:upper>]: std::mem::MaybeUninit<$typ> = std::mem::MaybeUninit::uninit();

            #[doc(hidden)]
            #[ctor::ctor]
            fn [<init_ $of:lower _ $name _flag>]() {
                unsafe {
                    [<$of:upper _ FLAG_ $name:upper>].as_mut_ptr().write($default_value);
                    $crate::utils::flags::[<register_ $typ:lower>]::<$of>(
                        [<$of:upper _ FLAG_ $name:upper>].as_mut_ptr().cast(),
                        stringify!($name),
                        $default_value,
                        $comment,
                        Some($short),
                    );
                }
            }

            pub fn $name() -> &'static $typ {
                unsafe { [<$of:upper _ FLAG_ $name:upper>].assume_init_ref() }
            }

            pub fn [<set_ $name>]($name: $typ) {
                unsafe {
                    *[<$of: upper _ FLAG_ $name:upper>].as_mut_ptr() = $name;
                }
            }


            pub fn [<is_ $of: lower _ $name _set>]() -> bool {
                $crate::utils::flags::FlagsOf::<$of>::is_set(stringify!($name))
            }
        }
    };
}

#[macro_export]
macro_rules! define_flag_handler {
    ($of: ident => $handler: expr, $name: ident, $comment: literal) => {
        paste::paste! {

            #[doc(hidden)]
            #[ctor::ctor]
            fn [<init_ $of: lower _ $name _flag>]() {
                $crate::utils::flags::register_flag_handler::<$of>($handler, stringify!($name), $comment, None);
            }


            pub fn [<is_ $of: lower _ $name _set>]() -> bool {
                $crate::utils::flags::FlagsOf::<$of>::is_set(stringify!($name))
            }
        }
    };
}

#[macro_export]
macro_rules! define_option_handler {
    ($of: ident => $handler: expr, $name: ident, $comment: literal) => {
        paste::paste! {

            #[doc(hidden)]
            #[ctor::ctor]
            fn [<init_ $of:lower _ $name _flag>]() {
                $crate::utils::flags::register_handler::<$of>($handler, stringify!($name), $comment, None);
            }


            pub fn [<is_ $of: lower _ $name _set>]() -> bool {
                $crate::utils::flags::FlagsOf::<$of>::is_set(stringify!($name))
            }
        }
    };
}

#[derive(Debug)]
pub enum FlagError {
    FlagNotFound(String),
    FlagsAlreadyInitialized(&'static str),
    NoFlags(&'static str),
}

pub struct VMKitFlags;
