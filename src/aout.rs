use glenda::error::Error;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct ExecHeader {
    pub magic: u32,
    pub text: u32,
    pub data: u32,
    pub bss: u32,
    pub syms: u32,
    pub entry: u32,
    pub spsz: u32,
    pub pcsz: u32,
}

pub const HDR_MAGIC: u32 = 0x00008000;
pub const R_MAGIC: u32 = HDR_MAGIC | (((4 * 28) * 28) + 7); // arm64: 0x8c47

impl ExecHeader {
    pub fn parse(buf: &[u8]) -> Result<Self, Error> {
        if buf.len() < 32 {
            return Err(Error::InvalidArgs);
        }
        let mut header = Self::default();
        header.magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        header.text = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        header.data = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        header.bss = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]);
        header.syms = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);
        header.entry = u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]);
        header.spsz = u32::from_be_bytes([buf[24], buf[25], buf[26], buf[27]]);
        header.pcsz = u32::from_be_bytes([buf[28], buf[29], buf[30], buf[31]]);
        
        if header.magic != R_MAGIC {
            return Err(Error::InvalidType);
        }
        
        Ok(header)
    }
}
