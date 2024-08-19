# swapstack

`SWAPSTACK()` primitive implementation. This operation allows unbinding thread from one stack and binding to another, kind of like `setcontext()` from `ucontext` but with more freedom and less overhead. The overall implementation is based on [libyugong](https://gitlab.anu.edu.au/kunshanwang/libyugong) and [Hop, Skip, & Jump: Practical On-Stack Replacement](https://dl.acm.org/doi/10.1145/3296975.3186412).

# Where is FrameCursor and other stuff for OSR?

OSR is implemented in vmkit crate together with other unwinding features. This crate *only* provides ability to switch stacks, nothing more.