# mmtk-liballoc

MMTk wrapper which allows to use it from any Rust code without too much hassle of implementing VMBinding trait, we do it for you!

# What mmtk-liballoc provides
- Stop the world thread system implementation
- VMBinding and other related traits implemented by us
- Fast-paths on allocation and write-barriers
- Easy to use: just implement `Trace` to trace an object, `Finalize` to finalize and so on
- Thread roots management through usage of shadow-stack, facilities to support Cranelift stackmaps
