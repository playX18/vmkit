.text 
.globl swapstack
.type swapstack, %function
.align 8 
swapstack:
    addi sp, sp, -0xd0

        # save fs0 - fs11
    fsd  fs0, 0x00(sp)
    fsd  fs1, 0x08(sp)
    fsd  fs2, 0x10(sp)
    fsd  fs3, 0x18(sp)
    fsd  fs4, 0x20(sp)
    fsd  fs5, 0x28(sp)
    fsd  fs6, 0x30(sp)
    fsd  fs7, 0x38(sp)
    fsd  fs8, 0x40(sp)
    fsd  fs9, 0x48(sp)
    fsd  fs10, 0x50(sp)
    fsd  fs11, 0x58(sp)

    # save s0-s11, ra
    sd  s0, 0x60(sp)
    sd  s1, 0x68(sp)
    sd  s2, 0x70(sp)
    sd  s3, 0x78(sp)
    sd  s4, 0x80(sp)
    sd  s5, 0x88(sp)
    sd  s6, 0x90(sp)
    sd  s7, 0x98(sp)
    sd  s8, 0xa0(sp)
    sd  s9, 0xa8(sp)
    sd  s10, 0xb0(sp)
    sd  s11, 0xb8(sp)
    sd  ra, 0xc0(sp)

    # save RA as PC
    sd  ra, 0xc8(sp)

    # store SP (pointing to context-data) in A2
    mv  a2, sp

    # restore SP (pointing to context-data) from A0
    mv  sp, a0
    mv  a0, a2 
    
    # load pc 
    ld a2, 0xc8(sp)
    jr a2