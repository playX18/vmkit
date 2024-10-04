
/*
    swapstack(
        from: *mut Stack (%rdi),
        to: *mut Stack (%rsi)
        data: %rdx
    )
*/
.text 
.globl swapstack
.type swapstack, @function
.align 8 
swapstack: 
    push %rbp
    mov  %rsp, %rbp

    push %rbx 
    push %r12 
    push %r13 
    push %r14 
    push %r15 

    movq %rdi, %rax 
    lea __swapstack_cont_local(%rip), %r11
    push %r11
    mov %rsp, (%rdi)
    mov (%rsi), %rsp 
    ret

/* ontop_swapstack(
    swapper: *mut Stack (%rdi),
    swappee: *mut Stack (%rsi),
    data: *mut () (%rdx),
    f: fn(Transfer) -> Transfer (%rcx)
) */
.text 
.globl ontop_swapstack
.type ontop_swapstack, @function  
.align 8 
ontop_swapstack: 
    push %rbp 
    mov %rsp, %rbp 
    push %rbx 
    push %r12 
    push %r13 
    push %r14 
    push %r15 


    lea __swapstack_cont_local(%rip), %r9
    push %r9
    mov %rsp, (%rdi)
    mov (%rsi), %rsp
    mov %rdx, %rsi
    call *%rcx /* call ontop function */
    ret


.text
.globl swapstack_cont 
.type swapstack_cont, @function
.align 8  
swapstack_cont:
__swapstack_cont_local:
    pop %r15 
    pop %r14 
    pop %r13 
    pop %r12 
    pop %rbx 

    pop %rbp 
    ret 

.text
.globl swapstack_begin_resume 
.type swapstack_begin_resume, @function
.align 8 
swapstack_begin_resume:
    mov %rax, %rdi
    mov %rdx, %rsi 
    ret 