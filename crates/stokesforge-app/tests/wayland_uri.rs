//! Guard of the transitional patch #72, without a graphical server.
//! The included file is the one compiled by winit: no second parser.

#![cfg(target_os = "linux")]
#[path = "../../../vendor/smithay-clipboard/src/file_drop_uri.rs"]
mod uri_list;
