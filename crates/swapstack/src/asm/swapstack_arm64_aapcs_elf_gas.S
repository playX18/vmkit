.text 
.globl swapstack
.type swapstack, %function
.align 8 
swapstack: 
    sub sp, sp, #0xb0
    adr x4, swapstack_cont
    str x4, [sp, #0x00]

    stp  d8,  d9,  [sp, #0x10]
    stp  d10, d11, [sp, #0x20]
    stp  d12, d13, [sp, #0x30]
    stp  d14, d15, [sp, #0x40]

    stp  x19, x20, [sp, #0x50]
    stp  x21, x22, [sp, #0x60]
    stp  x23, x24, [sp, #0x70]
    stp  x25, x26, [sp, #0x80]
    stp  x27, x28, [sp, #0x90]
    stp  x29, x30, [sp, #0xa0]

    // save my sp 
    mov x4, sp
    str x4, [x0]

    // set sp to swapee sp 
    ldr x4, [x1]
    mov sp, x4 
    
    // move x2 to result register
    // x0 = stack, x1 = data
    mov x1, x2
    // resume from the continue point of the opposite stack
    ldr x30, [sp, #0x00]
    br x30 

.text 
.globl ontop_swapstack
.type ontop_swapstack, @function  
.align 8 
ontop_swapstack: 
    sub sp, sp, #0xb0
    adr x4, swapstack_cont
    str x4, [sp, #0x00]

    stp  d8,  d9,  [sp, #0x10]
    stp  d10, d11, [sp, #0x20]
    stp  d12, d13, [sp, #0x30]
    stp  d14, d15, [sp, #0x40]

    stp  x19, x20, [sp, #0x50]
    stp  x21, x22, [sp, #0x60]
    stp  x23, x24, [sp, #0x70]
    stp  x25, x26, [sp, #0x80]
    stp  x27, x28, [sp, #0x90]
    stp  x29, x30, [sp, #0xa0]

    // save my sp 
    mov x4, sp
    str x4, [x0]

    // set sp to swapee sp 
    ldr x4, [x1]
    mov sp, x4 
    
    // move x2 to result register
    // x0 = stack, x1 = data
    mov x1, x2
    // resume from the continue point of the opposite stack
    ldr x30, [sp, #0x00]
    ret x3 // jump to ontop-function

.text
.globl swapstack_cont 
.type swapstack_cont, @function
.align 8  
swapstack_cont:
    ldp  d8,  d9,  [sp, #0x10]
    ldp  d10, d11, [sp, #0x20]
    ldp  d12, d13, [sp, #0x30]
    ldp  d14, d15, [sp, #0x40]

    ldp  x19, x20, [sp, #0x50]
    ldp  x21, x22, [sp, #0x60]
    ldp  x23, x24, [sp, #0x70]
    ldp  x25, x26, [sp, #0x80]
    ldp  x27, x28, [sp, #0x90]
    ldp  x29, x30, [sp, #0xa0]

    // Restore sp to callee sp
    add  sp, sp, #0xb0

    // Resume from the continue point of the opposite stack
    ret  x30

.text
.globl swapstack_begin_resume 
.type swapstack_begin_resume, @function
.align 8 
swapstack_begin_resume:
    ldp x9, x30, [sp], 16 
    br x9