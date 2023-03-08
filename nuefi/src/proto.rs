//! Defines the supported/known UEFI Protocols
//!
//! UEFI Protocols are how you interact with UEFI firmware, and how firmware
//! interacts with you. Protocols are interface pointers identified by a GUID.
//!
//! Currently, only a subset of the UEFI API is implemented.
//!
//! # Example
//!
//! Locate a protocol
//!
//! ```rust
//! use nuefi::{entry, EfiHandle, SystemTable, Boot, error, error::EfiStatus};
//! use nuefi::proto::graphics::GraphicsOutput;
//!
//! // Generate the UEFI entry point
//! #[entry()]
//! fn efi_main(handle: EfiHandle, table: SystemTable<Boot>) -> error::Result<()> {
//!     // Get UEFI Boot Services
//!     let boot = table.boot();
//!
//!     // Locate a handle for the `GraphicsOutput` Protocol
//!     let gop = boot.handle_for::<GraphicsOutput>()?;
//!
//!     // And then try to open the protocol for it
//!     let gop = boot.open_protocol::<GraphicsOutput>(gop)?;
//!     match gop {
//!         Some(proto) => {
//!             // Do something
//!         }
//!         None => {
//!             // Return an error if the Protocol does not exist.
//!             // For example, if the device doesn't have a screen.
//!             return Err(EfiStatus::UNSUPPORTED.into());
//!         }
//!     };
//!     Ok(())
//! }
//! #
//! # fn main() {}
//! ```

use core::{marker::PhantomData, ops::Deref};

use log::error;
use nuuid::Uuid;

use crate::{get_boot_table, EfiHandle};

pub mod console;
pub mod device_path;
pub mod edid;
pub mod graphics;
pub mod loaded_image;
pub mod media;
pub mod platform_init;
pub mod vendor;

pub use nuefi_core::base::{Char16, Guid};

pub type Str16 = *const Char16;

/// Defines a UEFI Protocol
///
/// See [`crate::Protocol`] for how to implement this.
/// This is the only safe way to implement this trait.
///
/// # Safety
///
/// This trait is unsafe because an incorrect GUID will
/// lead to type confusion and unsafety for both Rust and UEFI.
pub unsafe trait Protocol<'table> {
    /// Protocol GUID
    const GUID: Guid;

    /// Protocol Name
    const NAME: &'static str;

    /// Raw type of this Protocol
    type Raw;

    /// # Safety
    ///
    /// - Must be a valid, non-null, pointer to an instance of Self::Raw
    #[doc(hidden)]
    unsafe fn from_raw(this: *mut Self::Raw) -> Self;

    fn guid(&self) -> Guid {
        Self::GUID
    }
}

/// A scope around a [Protocol] that will call
/// [`crate::table::BootServices::close_protocol`] on [Drop]
#[derive(Debug)]
pub struct Scope<'table, Proto: Protocol<'table>> {
    proto: Proto,
    phantom: PhantomData<&'table mut Proto>,
    handle: EfiHandle,
    agent: EfiHandle,
    controller: Option<EfiHandle>,
}

impl<'table, Proto: Protocol<'table>> Scope<'table, Proto> {
    pub fn new(
        proto: Proto,
        handle: EfiHandle,
        agent: EfiHandle,
        controller: Option<EfiHandle>,
    ) -> Self {
        Self {
            proto,
            phantom: PhantomData,
            handle,
            agent,
            controller,
        }
    }

    /// Close this protocol
    pub fn close(self) {}

    /// "Leak" this Protocol
    ///
    /// It can be closed by calling
    /// [`crate::table::BootServices::close_protocol`]
    pub fn leak(self) {
        core::mem::forget(self);
    }
}

impl<'table, Proto: Protocol<'table>> Deref for Scope<'table, Proto> {
    type Target = Proto;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.proto
    }
}

impl<'table, Proto: Protocol<'table>> Drop for Scope<'table, Proto> {
    fn drop(&mut self) {
        if let Some(table) = get_boot_table() {
            let boot = table.boot();
            if let Err(e) = boot.close_protocol::<Proto>(self.handle, self.agent, self.controller) {
                error!("Error dropping scoped protocol: {e}");
            }
        } else {
            error!("Tried dropping scoped protocol after boot services");
        }
    }
}

/// UEFI Time information
///
///
/// Defined at <https://uefi.org/specs/UEFI/2.10/08_Services_Runtime_Services.html#gettime>
#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct Time {
    /// 1900 - 9999
    pub year: u16,

    /// 1 - 12
    pub month: u8,

    /// 1 - 31
    pub day: u8,

    /// 0 - 23
    pub hour: u8,

    /// 0 - 59
    pub minute: u8,

    /// 0 - 59
    pub second: u8,

    pub _pad1: u8,

    /// 0 - 999,999,999
    pub nanosecond: u32,

    /// —1440 to 1440 or 2047
    pub time_zone: i16,

    pub daylight: u8,

    pub _pad2: u8,
}

/// Identifies an entity within UEFI, such as a [`Protocol`].
///
/// You shouldn't need to use this, see more
/// specific traits like [`Protocol`] instead.
///
/// See the [`crate::GUID`] macro for how to implement this.
///
/// # Safety
///
/// This trait is unsafe because if the GUID you provide is wrong, UB can
/// result.
///
/// You must ensure the GUID is correct for whatever entity you are
/// representing, or else you will cause type confusion.
// #[doc(hidden)]
pub unsafe trait Entity {
    /// Entity GUID
    const GUID: Guid;

    /// Entity Name
    const NAME: &'static str;

    /// Entity Name
    fn name() -> &'static str {
        Self::NAME
    }

    /// Entity GUID
    fn guid() -> Guid {
        Self::GUID
    }
}
