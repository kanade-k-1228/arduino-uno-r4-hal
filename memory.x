MEMORY {
    FLASH : ORIGIN = 0x00000000, LENGTH = 256K
    RAM   : ORIGIN = 0x20000000, LENGTH = 32K
}

/* Option-Setting Memory has to be placed at this specific address. */
SECTIONS {
    PROVIDE(_option_setting = 0x00000400);
    .option_setting _option_setting : {
        __option_settting = .;
        KEEP(*(.option_setting));
        FILL(0xFFFFFFFF);
        . = _option_setting + 0x40;
    } > FLASH
}
INSERT AFTER .vector_table

/* Push .text section abit forward. */
PROVIDE(_stext = ADDR(.option_setting) + SIZEOF(.option_setting));
