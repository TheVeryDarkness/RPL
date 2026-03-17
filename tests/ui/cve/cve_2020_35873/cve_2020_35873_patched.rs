//@check-pass
#![allow(dangling_pointers_from_temporaries)]
use std::ptr;

struct Session<'a> {
    sess: *mut ffi::Session,
    _f: &'a (),
}

fn str_to_cstring(s: &str) -> Result<std::ffi::CString, Error> {
    std::ffi::CString::new(s).map_err(|_| Error)
}

struct Error;

macro_rules! check {
    ($expr:expr) => {{
        let ret = $expr;
        if ret != 0 {
            return Err(Error);
        }
    }};
}

impl Session<'_> {
    //#[rpl::dump_mir(dump_cfg, dump_ddg)]
    pub fn attach(&mut self, table: Option<&str>) -> Result<(), Error> {
        let table = if let Some(table) = table {
            Some(str_to_cstring(table)?)
        } else {
            None
        };
        let table = table.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());
        unsafe { check!(ffi::sqlite3session_attach(self.sess, table)) };
        Ok(())
    }
}

mod ffi {
    pub type Session = std::ffi::c_void;
    unsafe extern "C" {
        pub fn sqlite3session_attach(s: *mut Session, table: *const std::ffi::c_char) -> i32;
    }
}

fn main() {}
