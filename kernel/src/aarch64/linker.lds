SECTIONS {
  _phys_base = 0x40000000; /* QEMU virt load address */
  _virt_base = 0xffffff0000000000 + _phys_base;

  _dtb_start = _virt_base;
  . = _virt_base + 0x100000; /* leave space for DTB */

  /* start of the binary */
  .text : AT(ADDR(.text) - _virt_base + _phys_base) {
      _text_start = .;
      _text_start_phys = . - _virt_base + _phys_base;
      KEEP(*(.text.init)) /* from init.S */
      *(.text .text.* .gnu.linkonce.t*)
      _text_end = .;
      _text_end_phys = . - _virt_base + _phys_base;
  }

  . = ALIGN(4096);

  .rodata : AT(ADDR(.rodata) - _virt_base + _phys_base) {
    _rodata_start = .;
    _rodata_start_phys = . - _virt_base + _phys_base;
    *(.rodata .rodata.* .gnu.linkonce.r*)
    _rodata_end = .;
    _rodata_end_phys = . - _virt_base + _phys_base;
  }

  . = ALIGN(4096);

  .data : AT(ADDR(.data) - _virt_base + _phys_base) {
    _data_start = .;
    _data_start_phys = . - _virt_base + _phys_base;
    *(.data .data.* .gnu.linkonce.d* .got .got.plt)
    _data_end = .;
    _data_end_phys = . - _virt_base + _phys_base;
  }

  . = ALIGN(8);
  .bss (NOLOAD) : {
    _bss_start = .;
    _bss_start_phys = . - _virt_base + _phys_base;
    *(.bss .bss.*)
    *(COMMON)
    . = ALIGN(8);
    _bss_end = .;
    _bss_end_phys = . - _virt_base + _phys_base;
  }
  . = ALIGN(8);
  .initstack (NOLOAD) : {
    _initstack_start = .;
    _initstack_start_phys = . - _virt_base + _phys_base;
    . = . + 0x8000;
    _initstack_end = .;
    _initstack_end_phys = . - _virt_base + _phys_base;
    _initstack_size = _initstack_end - _initstack_start;
  }

  /* end of the binary */
  _end = ALIGN(8);

  /DISCARD/ : { *(.comment) *(.gnu*) *(.note*) *(.eh_frame*) }
}
