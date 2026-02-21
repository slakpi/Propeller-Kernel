__page_size = 4096;
__kernel_stack_pages = 2;
__kernel_page_tables = 16;
__vmsplit = 3;
__virtual_start = 0xc0000000;

EXTERN(vectors)

ENTRY(_start)

SECTIONS
{
  .text :
  {
    __kernel_start = .;

    KEEP(*(.text.boot))
    *(.text)
  }

  .rodata : ALIGN(__page_size)
  {
    *(.rodata .rodata.*)
  }

  .data : ALIGN(__page_size)
  {
    *(.data)
  }

  .bss : ALIGN(__page_size)
  {
    __bss_start = .;
    *(.bss .bss.*)
    __bss_end = .;
  }
  
  __bss_size = __bss_end - __bss_start;

  .data.stacks : ALIGN(__page_size)
  {
    . += (__kernel_stack_pages * __page_size);
    . += (__kernel_stack_pages * __page_size);
    . += (__kernel_stack_pages * __page_size);

    __kernel_svc_stack_end = .;
    . += (__kernel_stack_pages * __page_size);
    __kernel_svc_stack_start = .;
  }

  .data.stack_pointers : ALIGN(__page_size)
  {
    __kernel_stack_list = .;
    . += __page_size;
  }

  .text.vectors : ALIGN(__page_size)
  {
    __kernel_exception_vectors_start = .;

    KEEP(*(.text.vectors))

    . = ALIGN(__page_size);
    __kernel_exception_vectors_end = .;
  }

  .text.stubs : ALIGN(__page_size)
  {
    __kernel_exception_stubs_start = .;

    KEEP(*(.text.stubs))

    . = ALIGN(__page_size);
    __kernel_exception_stubs_end = .;
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
