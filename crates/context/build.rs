use std::fs;

use build_target::{target_os, Arch, Os};
macro_rules! p {
    ($($tokens: tt)*) => {
        println!("cargo:warning={}", format!($($tokens)*))
    }
}
fn main() {
    let target = build_target::target()
        .expect("unable to obtain target, please use ucontext or winfb features");

    let abi;
    if matches!(target.arch, Arch::AARCH64 | Arch::ARM) {
        abi = "aapcs";
    } else if target_os().unwrap() == Os::Windows {
        abi = "ms";
    } else if matches!(target.arch, Arch::MIPS | Arch::MIPS64) {
        if target.arch == Arch::MIPS {
            abi = "o32";
        } else {
            abi = "n64";
        }
    } else {
        abi = "sysv";
    }

    let arch = if matches!(target.arch, Arch::AARCH64) {
        "arm64"
    } else if matches!(target.arch, Arch::MIPS64) {
        "mips64"
    } else if matches!(target.arch, Arch::X86_64) {
        "x86_64"
    } else if matches!(target.arch, Arch::X86) {
        "i386"
    } else if matches!(target.arch, Arch::ARM) {
        "arm"
    } else if matches!(target.arch, Arch::MIPS) {
        "mips32"
    } else if matches!(target.arch, Arch::RISCV) {
        "riscv64"
    } else if matches!(target.arch, Arch::S390X) {
        "s390x"
    } else {
        panic!("not yet supported arch: {}", target.arch);
    };

    let mut cc = cc::Build::new();
    let tool = cc.get_compiler();

    let asm = if tool.is_like_msvc() {
        if matches!(target.arch, Arch::AARCH64 | Arch::ARM) {
            "armasm"
        } else {
            "masm"
        }
    } else {
        "gas"
    };

    let binfmt = if target.os == Os::Windows {
        "pe"
    } else if target.os == Os::MacOs {
        "mach-o"
    } else {
        "elf"
    };

    let ext = if asm == "pe" {
        ".asm"
    } else if asm == "gas" {
        ".S"
    } else {
        ".asm"
    };
    let binfmt = if binfmt == "mach-o" { "macho" } else { binfmt };

    let suffix = format!("_{}_{}_{}_{}{}", arch, abi, binfmt, asm, ext);

    let source_dir = fs::read_dir("src/asm").expect("failed to read asm sources");
    let mut cc = &mut cc;
    for entry in source_dir {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name.to_str().unwrap().ends_with(&suffix) {
            p!("compiling {}", name.to_str().unwrap());
            cc = cc.file(entry.path());
        }
    }

    cc.shared_flag(true);
    cc.compile("fcontext");

    println!("cargo::rerun-if-changed=build.rs");
}
