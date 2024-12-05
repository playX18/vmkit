use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use crate::{
    mm::scanning::{Tracer, Visitor},
    Runtime,
};

use super::GCVTable;

pub trait ScanSlots<R: Runtime> {
    fn scan(&self, visitor: &mut Visitor<R>) {
        let _ = visitor;
    }
}

pub trait TraceRefs<R: Runtime> {
    fn trace(&mut self, tracer: &mut Tracer<R>) {
        let _ = tracer;
    }
}

macro_rules! impl_no_trace {
    ($($t: ty)*) => {
        $(
            impl<R: Runtime> ScanSlots<R> for $t {
                fn scan(&self, _visitor: &mut Visitor<R>) {
                    // No-op
                }
            }

            impl<R: Runtime> TraceRefs<R> for $t {
                fn trace(&mut self, _tracer: &mut Tracer<R>) {
                    // No-op
                }
            }
        )*
        
    };
}


impl_no_trace! {
    // Primitive types
    u8 u16 u32 u64 u128 usize
    i8 i16 i32 i64 i128 isize
    f32 f64
    bool char

    // Standard library types
    String
    std::time::Duration
    std::time::Instant
    std::sync::atomic::AtomicBool
    std::sync::atomic::AtomicIsize
    std::sync::atomic::AtomicUsize
    std::sync::atomic::AtomicI8
    std::sync::atomic::AtomicU8
    std::sync::atomic::AtomicI16
    std::sync::atomic::AtomicU16
    std::sync::atomic::AtomicI32
    std::sync::atomic::AtomicU32
    std::sync::atomic::AtomicI64
    std::sync::atomic::AtomicU64


    std::thread::ThreadId

    std::ffi::CStr
    std::ffi::CString
    std::ffi::OsStr
    std::ffi::OsString
    std::path::Path
    std::path::PathBuf
    std::io::Error
    std::fs::File
    std::fs::OpenOptions
    std::fs::Metadata
    std::fs::DirEntry
    std::fs::ReadDir
    std::net::IpAddr
    std::net::Ipv4Addr
    std::net::Ipv6Addr
    std::net::SocketAddr
    std::net::SocketAddrV4
    std::net::SocketAddrV6
    std::net::TcpListener
    std::net::TcpStream
    std::net::UdpSocket
}


impl<R: Runtime, T: ScanSlots<R>> ScanSlots<R> for Vec<T> {
    fn scan(&self, visitor: &mut Visitor<R>) {
        for item in self {
            item.scan(visitor);
        }
    }
}

impl<R: Runtime, T: TraceRefs<R>> TraceRefs<R> for Vec<T> {
    fn trace(&mut self, tracer: &mut Tracer<R>) {
        for item in self {
            item.trace(tracer);
        }
    }
}

impl<R: Runtime, T: ScanSlots<R>> ScanSlots<R> for Option<T> {
    fn scan(&self, visitor: &mut Visitor<R>) {
        if let Some(item) = self {
            item.scan(visitor);
        }
    }
}

impl<R: Runtime, T: TraceRefs<R>> TraceRefs<R> for Option<T> {
    fn trace(&mut self, tracer: &mut Tracer<R>) {
        if let Some(item) = self {
            item.trace(tracer);
        }
    }
}

impl<R: Runtime, T: ScanSlots<R>> ScanSlots<R> for Box<T> {
    fn scan(&self, visitor: &mut Visitor<R>) {
        self.as_ref().scan(visitor);
    }
}

impl<R: Runtime, T: TraceRefs<R>> TraceRefs<R> for Box<T> {
    fn trace(&mut self, tracer: &mut Tracer<R>) {
        self.as_mut().trace(tracer);
    }
}

impl<R: Runtime, T: ScanSlots<R>> ScanSlots<R> for Arc<T> {
    fn scan(&self, visitor: &mut Visitor<R>) {
        self.as_ref().scan(visitor);
    }
}



impl<R: Runtime, K: ScanSlots<R>, V: ScanSlots<R>> ScanSlots<R> for HashMap<K, V> {
    fn scan(&self, visitor: &mut Visitor<R>) {
        for (key, value) in self {
            key.scan(visitor);
            value.scan(visitor);
        }
    }
}

impl<R: Runtime, K: ScanSlots<R>, V: ScanSlots<R>> ScanSlots<R> for BTreeMap<K, V> {
    fn scan(&self, visitor: &mut Visitor<R>) {
        for (key, value) in self {
            key.scan(visitor);
            value.scan(visitor);
        }
    }
}

impl<R: Runtime, T> ScanSlots<R> for std::marker::PhantomData<T> {
    fn scan(&self, _visitor: &mut Visitor<R>) {
        // No-op
    }
}

impl<R: Runtime, T> TraceRefs<R> for std::marker::PhantomData<T> {
    fn trace(&mut self, _tracer: &mut Tracer<R>) {
        // No-op
    }
}

/// A helper trait to convert type to length of an array. 
/// 
/// This is used mainly by `#[derive(Managed)]` macro from vmkit-derive crate. 
pub trait ArrayLikeLength {
    fn arraylike_length(&self) -> usize; 
}

impl ArrayLikeLength for u8 {
    fn arraylike_length(&self) -> usize {
        *self as usize
    }
}

impl ArrayLikeLength for u16 {
    fn arraylike_length(&self) -> usize {
        *self as usize
    }
}

impl ArrayLikeLength for u32 {
    fn arraylike_length(&self) -> usize {
        *self as usize
    }
}

impl ArrayLikeLength for u64 {
    fn arraylike_length(&self) -> usize {
        *self as usize
    }
}

impl ArrayLikeLength for u128 {
    fn arraylike_length(&self) -> usize {
        *self as usize
    }
}

impl ArrayLikeLength for usize {
    fn arraylike_length(&self) -> usize {
        *self
    }
}

impl ArrayLikeLength for i8 {
    fn arraylike_length(&self) -> usize {
        *self as usize
    }
}

impl ArrayLikeLength for i16 {
    fn arraylike_length(&self) -> usize {
        *self as usize
    }
}

impl ArrayLikeLength for i32 {
    fn arraylike_length(&self) -> usize {
        *self as usize
    }
}

impl ArrayLikeLength for i64 {
    fn arraylike_length(&self) -> usize {
        *self as usize
    }
}

impl ArrayLikeLength for i128 {
    fn arraylike_length(&self) -> usize {
        *self as usize
    }
}

impl ArrayLikeLength for isize {
    fn arraylike_length(&self) -> usize {
        *self as usize
    }
}

impl<R: Runtime, T, const N: usize> ScanSlots<R> for [T;N] {
    fn scan(&self, visitor: &mut Visitor<R>) {
        let _ = visitor;
    }
}

impl<R: Runtime, T, const N: usize> TraceRefs<R> for [T; N] {
    fn trace(&mut self, tracer: &mut Tracer<R>) {
        let _ = tracer;
    }
}

impl<R: Runtime, T> ScanSlots<R> for *mut T {}
impl<R:Runtime, T> ScanSlots<R> for *const T {}
impl<R: Runtime, T> TraceRefs<R> for *mut T {}
impl<R:Runtime, T> TraceRefs<R> for *const T {}


pub trait VTableDef<R: Runtime> {
    const VTABLE: GCVTable<R>;
}
