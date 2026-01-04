use core::fmt::Display;
use std::io::Write;

use serde::{Serialize, Serializer};
use serde_json::{
    Serializer as JSerializer,
    ser::{CompactFormatter, Compound},
};

pub use core::error::Error as StdError;
pub type DynStdError = dyn StdError + 'static;
pub type BoxedStdError = Box<dyn StdError + Send + Sync + 'static>;

pub fn grad_source(e: &DynStdError) -> Option<&DynStdError> {
    if let Some(e) = e.downcast_ref::<std::io::Error>() {
        e.get_ref().map(|x| x as &DynStdError)
    } else if let Some(e) = e.downcast_ref::<serde_json::Error>() {
        e.io_ref().map(|x| x as &DynStdError)
    } else if let Some(e) = e.downcast_ref::<http::Error>() {
        Some(e.get_ref())
    } else if let Some(e) = e.downcast_ref::<rand::rand_core::OsError>() {
        Some(&e.0)
    } else if let Some(e) = e.downcast_ref::<serde_path_to_error::Error<serde_json::Error>>() {
        Some(e.inner())
    } else if let Some(e) = e.downcast_ref::<serde_path_to_error::Error<serde::de::value::Error>>() {
        Some(e.inner())
    } else {
        e.source()
    }
}

pub fn serialize_err(mut err: &DynStdError) -> Vec<u8> {
    use serde::ser::SerializeSeq;

    let mut buf = Vec::with_capacity(128);
    let mut ser = JSerializer::new(&mut buf);
    let mut ss = unsafe { ser.serialize_seq(None).unwrap_unchecked() };

    loop {
        let _ = ss.serialize_element(unsafe {
            (core::ptr::from_ref(err) as *const TupleError).as_ref_unchecked()
        });

        let Some(e) = grad_source(err) else { break };
        err = e;
    }

    let _ = ss.end();
    buf
}

#[repr(transparent)]
struct TupleError(DynStdError);

impl Serialize for TupleError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeTuple;

        let mut tuple = serializer.serialize_tuple(2)?;
        tuple.serialize_element(
            #[cfg(feature = "build-std")]
            self.0.type_name(),
            #[cfg(not(feature = "build-std"))]
            &(),
        )?;
        tuple.collect_str(&self.0)?;
        tuple.end()
    }
}

trait SerializeTupleExt: serde::ser::SerializeTuple {
    fn collect_str<T>(&mut self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Display;
}

impl<S: serde::ser::SerializeTuple> SerializeTupleExt for S {
    default fn collect_str<T>(&mut self, _: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Display,
    {
        unimplemented!("Not implemented intentionally.");
    }
}

impl SerializeTupleExt for Compound<'_, &mut Vec<u8>, CompactFormatter> {
    fn collect_str<T>(mut self: &mut Self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Display,
    {
        match &mut self {
            Compound::Map { ser, .. } => {
                ser.as_inner().0.write_all(b",").map_err(serde_json::Error::io)?;
                ser.collect_str(value)
            }
        }
    }
}

#[inline]
#[must_use]
pub fn box_io_error(e: std::io::Error) -> BoxedStdError {
    if e.get_ref().is_some() {
        unsafe { e.into_inner().unwrap_unchecked() }
    } else {
        Box::new(e)
    }
}
