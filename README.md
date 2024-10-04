# vmkit
A library which provides bunch of building blocks to make a VM in Rust. 

# Feautures

- MMTK integration out of the box
- Thread management provided by vmkit, no need to manage thread-locals at all
- Custom thread stacks
- Swapstack primitive
- MacroAssembler framework which can be used to emit portable assembly or make JIT compilers


# Supported architectures and OSes

All Unixes should work out of the box, Windows should also work once MMTk adds support for it, although additional work on supporting it might be needed.

- x86-64: Full support provided
- aarch64: SWAPSTACK on its own should work but OSR most likely won't
