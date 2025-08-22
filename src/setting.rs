// Option Function Select Register 0
const OFS0: u32 = 0xFFFF_FFFF;
// Option Function Select Register 1
const OFS1: u32 = 0xFFFF_FFFF;

#[link_section = ".option_setting"]
#[no_mangle]
static __OPTION_SETTING: [u32; 2] = [
    OFS0,
    OFS1,
];
