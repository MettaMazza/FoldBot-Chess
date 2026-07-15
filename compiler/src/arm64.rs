// ARM64 (AArch64) instruction encoder for Ernos native code backend
// All instructions are 32-bit (4 bytes), little-endian

pub struct Arm64Encoder {
    pub code: Vec<u8>,
}

// Condition codes for B.cond
pub const COND_EQ: u8 = 0;
pub const COND_NE: u8 = 1;
pub const COND_LT: u8 = 0xB;
pub const COND_GE: u8 = 0xA;
pub const COND_LE: u8 = 0xD;
pub const COND_GT: u8 = 0xC;

impl Arm64Encoder {
    pub fn new() -> Self {
        Arm64Encoder { code: Vec::new() }
    }

    pub fn current_offset(&self) -> usize {
        self.code.len()
    }

    fn emit(&mut self, inst: u32) {
        self.code.extend_from_slice(&inst.to_le_bytes());
    }

    // ---- Data Processing (Register) ----

    /// MOVZ Xd, #imm16, LSL #shift (shift must be 0, 16, 32, or 48)
    pub fn movz(&mut self, rd: u8, imm16: u16, shift: u8) {
        let hw = (shift / 16) as u32;
        let inst = (0b1_10_100101u32 << 23) | (hw << 21) | ((imm16 as u32) << 5) | (rd as u32);
        self.emit(inst);
    }

    /// MOVK Xd, #imm16, LSL #shift
    pub fn movk(&mut self, rd: u8, imm16: u16, shift: u8) {
        let hw = (shift / 16) as u32;
        let inst = (0b1_11_100101u32 << 23) | (hw << 21) | ((imm16 as u32) << 5) | (rd as u32);
        self.emit(inst);
    }

    /// ADD Xd, Xn, Xm
    pub fn add_reg(&mut self, rd: u8, rn: u8, rm: u8) {
        let inst = (0b1_00_01011_00_0u32 << 21) | ((rm as u32) << 16) | (0b000000u32 << 10) | ((rn as u32) << 5) | (rd as u32);
        self.emit(inst);
    }

    /// SUB Xd, Xn, Xm
    pub fn sub_reg(&mut self, rd: u8, rn: u8, rm: u8) {
        let inst = (0b1_10_01011_00_0u32 << 21) | ((rm as u32) << 16) | (0b000000u32 << 10) | ((rn as u32) << 5) | (rd as u32);
        self.emit(inst);
    }

    /// MUL Xd, Xn, Xm (alias for MADD Xd, Xn, Xm, XZR)
    pub fn mul_reg(&mut self, rd: u8, rn: u8, rm: u8) {
        // MADD: 1_00_11011_000_Rm_0_Ra_Rn_Rd, Ra=31 (XZR)
        let inst = (0b1_00_11011_000u32 << 21) | ((rm as u32) << 16) | (0u32 << 15) | (31u32 << 10) | ((rn as u32) << 5) | (rd as u32);
        self.emit(inst);
    }

    /// SDIV Xd, Xn, Xm
    pub fn sdiv(&mut self, rd: u8, rn: u8, rm: u8) {
        // 1_00_11010110_Rm_00001_1_Rn_Rd
        let inst = (0b1_00_11010110u32 << 21) | ((rm as u32) << 16) | (0b000011u32 << 10) | ((rn as u32) << 5) | (rd as u32);
        self.emit(inst);
    }

    /// ADD Xd, Xn, #imm12
    pub fn add_imm(&mut self, rd: u8, rn: u8, imm12: u16) {
        let inst = (0b1_00_100010_0u32 << 22) | ((imm12 as u32 & 0xFFF) << 10) | ((rn as u32) << 5) | (rd as u32);
        self.emit(inst);
    }

    /// SUB Xd, Xn, #imm12
    pub fn sub_imm(&mut self, rd: u8, rn: u8, imm12: u16) {
        let inst = (0b1_10_100010_0u32 << 22) | ((imm12 as u32 & 0xFFF) << 10) | ((rn as u32) << 5) | (rd as u32);
        self.emit(inst);
    }

    /// MOV Xd, Xm (alias for ORR Xd, XZR, Xm)
    pub fn mov_reg(&mut self, rd: u8, rm: u8) {
        let inst = (0b1_01_01010_00_0u32 << 21) | ((rm as u32) << 16) | (0b000000u32 << 10) | (31u32 << 5) | (rd as u32);
        self.emit(inst);
    }

    /// NEG Xd, Xm (alias for SUB Xd, XZR, Xm)
    pub fn neg(&mut self, rd: u8, rm: u8) {
        self.sub_reg(rd, 31, rm);
    }

    // ---- Load / Store ----

    /// STR Xt, [Xn, #offset] (unsigned offset, 8-byte aligned)
    pub fn str_imm(&mut self, rt: u8, rn: u8, offset: i16) {
        let uoff = (offset as u16 / 8) as u32;
        let inst = (0b11_111_00_100u32 << 22) | ((uoff & 0xFFF) << 10) | ((rn as u32) << 5) | (rt as u32);
        self.emit(inst);
    }

    /// LDR Xt, [Xn, #offset] (unsigned offset, 8-byte aligned)
    pub fn ldr_imm(&mut self, rt: u8, rn: u8, offset: i16) {
        let uoff = (offset as u16 / 8) as u32;
        let inst = (0b11_111_00_101u32 << 22) | ((uoff & 0xFFF) << 10) | ((rn as u32) << 5) | (rt as u32);
        self.emit(inst);
    }

    /// STP Xt1, Xt2, [Xn, #imm]! (pre-index, imm in bytes, must be 8-byte aligned)
    pub fn stp_pre(&mut self, rt1: u8, rt2: u8, rn: u8, imm_bytes: i16) {
        let imm7 = ((imm_bytes / 8) as i8 as u8 & 0x7F) as u32;
        let inst = (0b10_101_00_110u32 << 22) | (imm7 << 15) | ((rt2 as u32) << 10) | ((rn as u32) << 5) | (rt1 as u32);
        self.emit(inst);
    }

    /// LDP Xt1, Xt2, [Xn], #imm (post-index, imm in bytes, must be 8-byte aligned)
    pub fn ldp_post(&mut self, rt1: u8, rt2: u8, rn: u8, imm_bytes: i16) {
        let imm7 = ((imm_bytes / 8) as i8 as u8 & 0x7F) as u32;
        let inst = (0b10_101_00_011u32 << 22) | (imm7 << 15) | ((rt2 as u32) << 10) | ((rn as u32) << 5) | (rt1 as u32);
        self.emit(inst);
    }

    // ---- Branch ----

    /// BL offset (offset in bytes from the BL instruction, must be 4-byte aligned)
    pub fn bl(&mut self, offset_bytes: i32) {
        let imm26 = ((offset_bytes >> 2) as u32) & 0x03FFFFFF;
        let inst = (0b1_00101u32 << 26) | imm26;
        self.emit(inst);
    }

    /// B offset (unconditional branch)
    pub fn b(&mut self, offset_bytes: i32) {
        let imm26 = ((offset_bytes >> 2) as u32) & 0x03FFFFFF;
        let inst = (0b0_00101u32 << 26) | imm26;
        self.emit(inst);
    }

    /// B.cond offset
    pub fn b_cond(&mut self, cond: u8, offset_bytes: i32) {
        let imm19 = ((offset_bytes >> 2) as u32) & 0x7FFFF;
        let inst = (0b01010100u32 << 24) | (imm19 << 5) | (cond as u32);
        self.emit(inst);
    }

    /// RET (return to address in X30/LR)
    pub fn ret(&mut self) {
        self.emit(0xD65F03C0);
    }

    /// CMP Xn, Xm (alias for SUBS XZR, Xn, Xm)
    pub fn cmp_reg(&mut self, rn: u8, rm: u8) {
        let inst = (0b1_11_01011_00_0u32 << 21) | ((rm as u32) << 16) | (0b000000u32 << 10) | ((rn as u32) << 5) | 31u32;
        self.emit(inst);
    }

    /// CMP Xn, #imm12 (alias for SUBS XZR, Xn, #imm12)
    pub fn cmp_imm(&mut self, rn: u8, imm12: u16) {
        let inst = (0b1_11_100010_0u32 << 22) | ((imm12 as u32 & 0xFFF) << 10) | ((rn as u32) << 5) | 31u32;
        self.emit(inst);
    }

    // ---- PC-Relative ----

    /// ADRP Xd, page_offset (in 4KB pages)
    pub fn adrp(&mut self, rd: u8, page_offset: i32) {
        let imm = page_offset as u32;
        let immlo = imm & 0x3;
        let immhi = (imm >> 2) & 0x7FFFF;
        let inst = (1u32 << 31) | (immlo << 29) | (0b10000u32 << 24) | (immhi << 5) | (rd as u32);
        self.emit(inst);
    }

    /// ADR Xd, byte_offset
    pub fn adr(&mut self, rd: u8, offset: i32) {
        let imm = offset as u32;
        let immlo = imm & 0x3;
        let immhi = (imm >> 2) & 0x7FFFF;
        let inst = (0u32 << 31) | (immlo << 29) | (0b10000u32 << 24) | (immhi << 5) | (rd as u32);
        self.emit(inst);
    }

    // ---- Logical ----

    /// AND Xd, Xn, Xm
    pub fn and_reg(&mut self, rd: u8, rn: u8, rm: u8) {
        let inst = (0b1_00_01010_00_0u32 << 21) | ((rm as u32) << 16) | (0b000000u32 << 10) | ((rn as u32) << 5) | (rd as u32);
        self.emit(inst);
    }

    /// ORR Xd, Xn, Xm
    pub fn orr_reg(&mut self, rd: u8, rn: u8, rm: u8) {
        let inst = (0b1_01_01010_00_0u32 << 21) | ((rm as u32) << 16) | (0b000000u32 << 10) | ((rn as u32) << 5) | (rd as u32);
        self.emit(inst);
    }

    /// LSL Xd, Xn, Xm (alias for LSLV)
    pub fn lsl_reg(&mut self, rd: u8, rn: u8, rm: u8) {
        // LSLV: 1_00_11010110_Rm_0010_00_Rn_Rd
        let inst = (0b1_00_11010110u32 << 21) | ((rm as u32) << 16) | (0b001000u32 << 10) | ((rn as u32) << 5) | (rd as u32);
        self.emit(inst);
    }

    /// LSR Xd, Xn, Xm (alias for LSRV)
    pub fn lsr_reg(&mut self, rd: u8, rn: u8, rm: u8) {
        // LSRV: 1_00_11010110_Rm_0010_01_Rn_Rd
        let inst = (0b1_00_11010110u32 << 21) | ((rm as u32) << 16) | (0b001001u32 << 10) | ((rn as u32) << 5) | (rd as u32);
        self.emit(inst);
    }

    // ---- Helpers ----

    /// Load a full 64-bit immediate into register using MOVZ + up to 3 MOVK
    pub fn load_i64(&mut self, rd: u8, value: i64) {
        let v = value as u64;
        self.movz(rd, (v & 0xFFFF) as u16, 0);
        if v > 0xFFFF {
            self.movk(rd, ((v >> 16) & 0xFFFF) as u16, 16);
        }
        if v > 0xFFFFFFFF {
            self.movk(rd, ((v >> 32) & 0xFFFF) as u16, 32);
        }
        if v > 0xFFFFFFFFFFFF {
            self.movk(rd, ((v >> 48) & 0xFFFF) as u16, 48);
        }
    }

    /// NOP
    pub fn nop(&mut self) {
        self.emit(0xD503201F);
    }

    /// Patch a BL instruction at `offset` to branch to `target_offset`
    pub fn patch_bl(&mut self, offset: usize, target_offset: usize) {
        let rel = (target_offset as i32 - offset as i32) >> 2;
        let imm26 = (rel as u32) & 0x03FFFFFF;
        let inst = (0b1_00101u32 << 26) | imm26;
        let bytes = inst.to_le_bytes();
        self.code[offset..offset + 4].copy_from_slice(&bytes);
    }

    /// Patch a B instruction at `offset` to branch to `target_offset`
    pub fn patch_b(&mut self, offset: usize, target_offset: usize) {
        let rel = (target_offset as i32 - offset as i32) >> 2;
        let imm26 = (rel as u32) & 0x03FFFFFF;
        let inst = (0b0_00101u32 << 26) | imm26;
        let bytes = inst.to_le_bytes();
        self.code[offset..offset + 4].copy_from_slice(&bytes);
    }

    /// Patch a B.cond instruction at `offset` to branch to `target_offset`
    pub fn patch_b_cond(&mut self, offset: usize, target_offset: usize) {
        let old = u32::from_le_bytes([
            self.code[offset], self.code[offset+1], self.code[offset+2], self.code[offset+3],
        ]);
        let cond = old & 0xF;
        let rel = (target_offset as i32 - offset as i32) >> 2;
        let imm19 = (rel as u32) & 0x7FFFF;
        let inst = (0b01010100u32 << 24) | (imm19 << 5) | cond;
        let bytes = inst.to_le_bytes();
        self.code[offset..offset + 4].copy_from_slice(&bytes);
    }
}
