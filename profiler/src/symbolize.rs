//! Ties the kernel (`kallsyms`) and user-space (`usersym`) resolvers
//! together into a single "resolve this IP" facade, and carries the
//! kernel/user/JIT/unknown distinction through to the SVG renderer for
//! color-coding.

use crate::kallsyms::Kallsyms;
use crate::usersym::UserSymbolCache;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Kernel,
    User,
    Jit,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Frame {
    Kernel(String),
    User(String),
    Jit(String),
    Unknown,
}

impl Frame {
    pub fn label(&self) -> String {
        match self {
            Frame::Kernel(s) | Frame::User(s) | Frame::Jit(s) => s.clone(),
            Frame::Unknown => "[unknown]".to_string(),
        }
    }

    pub fn kind(&self) -> FrameKind {
        match self {
            Frame::Kernel(_) => FrameKind::Kernel,
            Frame::User(_) => FrameKind::User,
            Frame::Jit(_) => FrameKind::Jit,
            Frame::Unknown => FrameKind::Unknown,
        }
    }
}

pub fn resolve_kernel(kallsyms: &Kallsyms, ip: u64) -> Frame {
    match kallsyms.resolve(ip) {
        Some((name, 0)) => Frame::Kernel(name.to_string()),
        Some((name, off)) => Frame::Kernel(format!("{name}+0x{off:x}")),
        None => Frame::Unknown,
    }
}

pub fn resolve_user(usersyms: &mut UserSymbolCache, pid: u32, ip: u64) -> Frame {
    if let Some((name, off)) = usersyms.resolve(pid, ip) {
        return labeled(Frame::User, name, off);
    }
    // Anonymous mappings (JIT-compiled code) have no ELF symbol table for
    // `resolve` to find; fall back to a JIT engine's own map file.
    if let Some((name, off)) = usersyms.resolve_jit(pid, ip) {
        return labeled(Frame::Jit, name, off);
    }
    Frame::Unknown
}

fn labeled(ctor: fn(String) -> Frame, name: String, offset: u64) -> Frame {
    if offset == 0 {
        ctor(name)
    } else {
        ctor(format!("{name}+0x{offset:x}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_label_includes_offset_when_nonzero() {
        assert_eq!(Frame::Kernel("do_idle".into()).label(), "do_idle");
        assert_eq!(Frame::User("main+0x10".into()).label(), "main+0x10");
        assert_eq!(Frame::Jit("LazyCompile:*foo".into()).label(), "LazyCompile:*foo");
        assert_eq!(Frame::Unknown.label(), "[unknown]");
    }

    #[test]
    fn frame_kind_matches_variant() {
        assert_eq!(Frame::Kernel("x".into()).kind(), FrameKind::Kernel);
        assert_eq!(Frame::User("x".into()).kind(), FrameKind::User);
        assert_eq!(Frame::Jit("x".into()).kind(), FrameKind::Jit);
        assert_eq!(Frame::Unknown.kind(), FrameKind::Unknown);
    }
}
