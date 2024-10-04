.code 

swapstack PROC FRAME
    .endprolog

    ; prepare stack 
    push rbp
    mov rbp, rsp 

    push rbx
    push rsi 
    push rdi 
    push r12 
    push r13
    push r14 
    push r15

    push rcx ; save hidden address of Transfer 
    lea swapstack_cont, r11 
    push r11 
    mov [rdx], rsp  
    mov rsp, [rdx]
    ret 
swapstack ENDP 
END

