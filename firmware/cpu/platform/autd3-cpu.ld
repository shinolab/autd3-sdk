OUTPUT_FORMAT("elf32-littlearm", "elf32-bigarm", "elf32-littlearm")
OUTPUT_ARCH(arm)
ENTRY("_PowerON_Reset")

MEMORY
{
	ATCM(wax)        : org = 0x00000000, len = 0x00070000
	ATCM_STACK(rw)   : org = 0x00070000, len = 0x00010000
	BTCM(wax)        : org = 0x00800000, len = 0x00008000
	SERIAL_FLASH(rx) : org = 0x30000000, len = 0x00010000
	APP_FLASH(rx)    : org = 0x30010100, len = 0x0007FF00
}

SECTIONS
{
	_sflash_top = 0x30000000;
	_app_image_s = ORIGIN(APP_FLASH);

	.loader_param :
	{
		_loader_param_ram_s = .;
		KEEP(* (.loader_param))
		_loader_param_ram_e = .;
	} > BTCM AT > SERIAL_FLASH

	.loader_text 0x00802000 :
	{
		_loader_text_ram_s = .;
		KEEP(* (.loader_text))
		KEEP(* (.loader_text2))
		_loader_text_ram_e = .;
	} > BTCM AT > SERIAL_FLASH

	.vector :
	{
		_vec_ram_s = .;
		KEEP(* (.fvectors))
		_vec_ram_e = .;
	} > ATCM AT > APP_FLASH

	_vec_rom_s = LOADADDR(.vector);

	.text 0x00040000 :
	{
		_text = .;
		* (.text)
		* (.text.*)
		. = ALIGN(8);
		_etext = .;
	} > ATCM AT > APP_FLASH

	_mtext = LOADADDR(.text);

	.rodata :
	{
		_rodata = .;
		* (.rodata)
		* (.rodata.*)
		. = ALIGN(8);
		_erodata = .;
	} > ATCM AT > APP_FLASH

	_mrodata = LOADADDR(.rodata);

	.data :
	{
		_data = .;
		* (.data)
		* (.data.*)
		. = ALIGN(8);
		_edata = .;
	} > ATCM AT > APP_FLASH

	_mdata = LOADADDR(.data);

	.bss :
	{
		_bss = .;
		* (.bss)
		* (.bss.*)
		* (COMMON)
		_ebss = .;
	} > ATCM

	.cstack 0x00070000 :
	{
		_cstack_s = .;
		. += 0x2000;
		* (cstack)
		_sys_stack = _cstack_s + 0x2000;
		_stack = .;
	} > ATCM_STACK

	.svc_stack (0x00070000 + 0x2000) :
	{
		_svc_stack_s = .;
		. += 0x2000;
		* (svc_stack)
		_svc_stack = _svc_stack_s + 0x2000;
	} > ATCM_STACK

	.irq_stack (0x00070000 + 0x4000) :
	{
		_irq_stack_s = .;
		. += 0x2000;
		* (irq_stack)
		_irq_stack = _irq_stack_s + 0x2000;
	} > ATCM_STACK

	.fiq_stack (0x00070000 + 0x6000) :
	{
		_fiq_stack_s = .;
		. += 0x1000;
		* (fiq_stack)
		_fiq_stack = _fiq_stack_s + 0x1000;
	} > ATCM_STACK

	.und_stack (0x00070000 + 0x7000) :
	{
		_und_stack_s = .;
		. += 0x1000;
		* (und_stack)
		_und_stack = _und_stack_s + 0x1000;
	} > ATCM_STACK

	.abt_stack (0x00070000 + 0x8000) :
	{
		_abt_stack_s = .;
		. += 0x1000;
		* (abt_stack)
		_abt_stack = _abt_stack_s + 0x1000;
	} > ATCM_STACK

	.heap (0x00070000 + 0x9000) :
	{
		_heap_s = .;
		. += 0x1000;
		* (heap)
		_heap_e = _heap_s + 0x1000;
	} > ATCM_STACK

	/DISCARD/ :
	{
		*(.ARM.exidx*)
		*(.ARM.extab*)
		*(.eh_frame*)
	}

	ASSERT(SIZEOF(.vector) == 0x40,
		"the vector table must stay 0x40 bytes: the loader copies it as a fixed-size block")
	ASSERT(LOADADDR(.vector) == ORIGIN(APP_FLASH),
		"the vector table must be the first block of the slot image")
	ASSERT(ADDR(.text) == 0x00040000,
		"the application must be loaded at 0x00040000: the loader copies it to a fixed address")
	ASSERT(LOADADDR(.text) == ORIGIN(APP_FLASH) + SIZEOF(.vector),
		"the application block must follow the vector table with no gap")
	ASSERT(LOADADDR(.rodata) - ADDR(.rodata) == LOADADDR(.text) - ADDR(.text),
		".rodata must keep the load-to-run offset of .text: the loader copies one flat block")
	ASSERT(LOADADDR(.data) - ADDR(.data) == LOADADDR(.text) - ADDR(.text),
		".data must keep the load-to-run offset of .text: the loader copies one flat block")
}
