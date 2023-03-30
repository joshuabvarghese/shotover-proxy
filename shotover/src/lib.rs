//! The data layer proxy.
//!
//! Below are the main areas that you should be looking at to extend and work with Shotover.
//!
//! Creating a transform is largely just implementing the [`transforms::Transform`] trait and registering it with
//! the [`transforms::Transforms`] enum.
//!
//! To allow your [`transforms::Transform`] to be configurable in Shotover config files you will need to create
//! a serializable config struct and implement [`transforms::TransformConfig`] trait.
//!
//! ## Messages
//! * [`message::Message`], the main struct that carries database queries/frames around in Shotover.
//!
//! ## Transform
//! * [`transforms::Wrapper`], used to wrap messages as they traverse the [`transforms::Transform`] chain.
//! * [`transforms::Transform`], the main extension trait for adding your own behaviour to Shotover.
//! * [`transforms::Transforms`], the enum to register with (add a variant) for enabling your own transform.
//! * [`transforms::TransformConfig`], the trait to implement for configuring your own transform.

// Accidentally printing would break json log output
#![deny(clippy::print_stdout)]
#![deny(clippy::print_stderr)]

pub mod codec;
pub mod config;
pub mod error;
pub mod frame;
pub mod message;
pub mod message_value;
mod observability;
pub mod runner;
mod server;
pub mod sources;
pub mod tcp;
pub mod tls;
pub mod tracing_panic_handler;
pub mod transforms;