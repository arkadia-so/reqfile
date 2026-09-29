//! The functional core: parsing, planning and judging, without any I/O.

pub mod cache;
pub mod command;
pub mod config;
pub mod conflict;
pub mod decision;
pub mod error;
pub mod examples;
pub mod junit;
pub mod paths;
pub mod pins;
pub mod plan;
pub mod pretty;
pub mod report;
pub mod reqfile;
pub mod resolve;
pub mod runlog;
pub mod sarif;
pub mod tags;
pub mod yaml;
