SECTIONS {
  /*. = 0xffffff8000080000;*/ /* Raspbery Pi 3 Aarch64 (kernel8.img) load address @ highmem */

  _ram_start = 0x40000000;
  _dtb_start = 0x40000000;
  . = 0x40100000; /* QEMU virt load address + leave space for DTB */

  /* start of the binary */
  .text : {
      _text_start = .;
      KEEP(*(.text.init)) /* from init.S */
      *(.text .text.* .gnu.linkonce.t*)
      _text_end = .;
  }

  . = ALIGN(4096);

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
  . = ALIGN(8);
  .initstack (NOLOAD) : {
    _initstack_start = .;
    . = . + 0x2000;
    _initstack_end = .;
    _initstack_size = _initstack_end - _initstack_start;
  }

  /* end of the binary */
  _end = ALIGN(8);

  /* number of bytes in BSS section and complete binary */
  /*__bss_length = (__bss_end - __bss_start);
  __binary_length = (_end - _start);*/

  /DISCARD/ : { *(.comment) *(.gnu*) *(.note*) *(.eh_frame*) }
}
