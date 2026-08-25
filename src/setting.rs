const OFS0: u32 = 0xFFFF_FFFF;
// HOCOFRQ1=0b100 (48 MHz) and HOCOEN=0. This matches the UNO R4 bootloader's
// RA4M1 option-setting value; 0xFFFF_FFFF would select the prohibited value 0b111.
const OFS1: u32 = 0xFFFF_CEDF;

#[link_section = ".option_setting"]
#[no_mangle]
static __OPTION_SETTING: [u32; 2] = [OFS0, OFS1];

#[cfg(test)]
mod tests {
    use super::OFS1;

    #[test]
    fn ofs1_selects_the_48_mhz_hoco() {
        assert_eq!((OFS1 >> 12) & 0b111, 0b100);
        assert_eq!((OFS1 >> 8) & 1, 0); // Start HOCO after reset.
    }
}
