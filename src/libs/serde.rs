use core::{fmt, time::Duration};
use std::{io, time::SystemTime};

use serde::{Serialize, Serializer, ser::SerializeMap};
use serde_json::Serializer as JSerializer;

#[repr(transparent)]
struct Adapter<'a, 'b: 'a> {
    #[cfg(feature = "build-std")]
    inner: &'a mut Vec<u8>,

    #[cfg(feature = "build-std")]
    phantom: core::marker::PhantomData<&'a mut fmt::Formatter<'b>>,

    #[cfg(not(feature = "build-std"))]
    inner: &'a mut fmt::Formatter<'b>,
}

impl<'a, 'b> Adapter<'a, 'b> {
    #[cfg(feature = "build-std")]
    #[inline]
    const fn new(inner: &'a mut fmt::Formatter<'b>) -> Self {
        Self {
            inner: unsafe { core::ptr::NonNull::from_mut(inner.inner()).cast().as_mut() },
            phantom: core::marker::PhantomData,
        }
    }

    #[cfg(not(feature = "build-std"))]
    #[inline]
    const fn new(inner: &'a mut fmt::Formatter<'b>) -> Self {
        Self { inner }
    }
}

impl io::Write for Adapter<'_, '_> {
    #[cfg(feature = "build-std")]
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.extend_from_slice(buf);
        Ok(buf.len())
    }

    #[cfg(not(feature = "build-std"))]
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.inner.write_str(str::from_utf8(buf).map_err(io::Error::other)?) {
            Ok(()) => Ok(buf.len()),
            Err(e) => Err(io::Error::other(e)),
        }
    }

    #[cfg(feature = "build-std")]
    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.inner.extend_from_slice(buf);
        Ok(())
    }

    #[cfg(not(feature = "build-std"))]
    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.inner.write_str(str::from_utf8(buf).map_err(io::Error::other)?).map_err(io::Error::other)
    }

    #[cfg(not(feature = "build-std"))]
    #[inline]
    fn write_fmt(&mut self, fmt: fmt::Arguments<'_>) -> io::Result<()> {
        self.inner.write_fmt(fmt).map_err(io::Error::other)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct JsonFormatArg<'a>(pub fmt::Arguments<'a>);

impl fmt::Display for JsonFormatArg<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let adapter = Adapter::new(f);
        let mut ser = JSerializer::new(adapter);
        ser.collect_str(&self.0).map_err(|_| fmt::Error)
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct WithJson<T>(pub T);

impl<T> fmt::Display for WithJson<T>
where
    T: Serialize,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let adapter = Adapter::new(f);
        let mut ser = JSerializer::new(adapter);
        self.0.serialize(&mut ser).map_err(|_| fmt::Error)
    }
}

#[derive(Serialize)]
pub struct UnitMap {}

#[repr(transparent)]
pub struct SliceMap<K, V>(pub [(K, V)]);

impl<K, V> SliceMap<K, V> {
    pub const fn from_slice(slice: &[(K, V)]) -> &Self {
        unsafe { core::mem::transmute(slice) }
    }
}

impl<K, V> Serialize for SliceMap<K, V>
where
    K: Serialize,
    V: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

#[inline]
pub fn JsDuration<S>(dur: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_u128(dur.as_millis())
}

#[inline]
pub fn JsTime<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    JsDuration(unsafe { &*core::ptr::from_ref(time).cast() }, serializer)
}
