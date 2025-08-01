SECTIONS {
  . = 0x10000000;

  /* start of the binary */
  .text : {
      _text_start = .;
      KEEP(*(.text.init)) /* from init.S */
      *(.text .text.* .gnu.linkonce.t*)
      _text_end = .;
  }

  /* TODO: NX bit is not implemented yet */
  /* . = ALIGN(4096); */

  .rodata : {
    _rodata_start = .;
    *(.rodata .rodata.* .gnu.linkonce.r*)
    _rodata_end = .;
  }

  . = ALIGN(4096);

  .data : {
    _data_start = .;
    *(.data .data.* .gnu.linkonce.d* .got .got.plt)
    _data_end = .;
  }

  . = ALIGN(8);
  .bss (NOLOAD) : {
    _bss_start = .;
    *(.bss .bss.*)
    *(COMMON)
    . = ALIGN(8);
    _bss_end = .;
  }

  /* end of the binary */
  _end = ALIGN(8);

  /DISCARD/ : { *(.comment) *(.gnu*) *(.note*) *(.eh_frame*) }
}
