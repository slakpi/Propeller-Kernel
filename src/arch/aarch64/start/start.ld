__page_size = 4096;
__kernel_stack_pages = 2;
__kernel_page_tables = 16;
__virtual_start = 0xffff000000000000;

ENTRY(_start)

SECTIONS
{
  .text :
  {
    __kernel_start = .;
    __text_start = .;

    KEEP(*(.text.boot))
    *(.text)

    . = ALIGN(__page_size);
    __text_end = .;
  }

  .rodata : ALIGN(__page_size)
  {
    __rodata_start = .;

    *(.rodata .rodata.*)

    . = ALIGN(__page_size);
    __rodata_end = .;
  }

  .data : ALIGN(__page_size)
  {
    __data_start = .;

    *(.data)

    . = ALIGN(__page_size);
    __data_end = .;
  }

  .bss : ALIGN(__page_size)
  {
    __bss_start = .;

    *(.bss .bss.*)

    . = ALIGN(__page_size);
    __bss_end = .;
  }

  __bss_size = __bss_end - __bss_start;

  .data.stack : ALIGN(__page_size)
  {
    __kernel_stack_end = .;
    . += (__kernel_stack_pages * __page_size);
    __kernel_stack_start = .;
  }

  .data.stack_pointers : ALIGN(__page_size)
  {
    __kernel_stack_list = .;
    . += __page_size;
  }

  __kernel_size = . - __kernel_start;

  .data.id_pages : ALIGN(__page_size)
  {
    __kernel_id_pages_start = .;
    . += (__kernel_page_tables * __page_size);
    __kernel_id_pages_end = .;
  }

  __kernel_id_pages_size = __kernel_id_pages_end - __kernel_id_pages_start;

  .data.pages : ALIGN(__page_size)
  {
    __kernel_pages_start = .;
    . += (__kernel_page_tables * __page_size);
    __kernel_pages_end = .;
  }

  __kernel_pages_size = __kernel_pages_end - __kernel_pages_start;

  __kernel_end = __kernel_pages_end;
}
