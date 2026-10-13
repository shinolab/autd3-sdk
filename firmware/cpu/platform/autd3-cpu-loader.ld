OUTPUT_ARCH(arm)
ENTRY(_PowerON_Reset)

MEMORY
{
	BTCM_PARAM(r)    : org = 0x00800000, len = 0x0000004C
	BTCM_LOADER(wax) : org = 0x00802000, len = 0x00006000
	SERIAL_FLASH(rx) : org = 0x30000000, len = 0x00010000
}

SECTIONS
{
	.loader_param :
	{
		KEEP(* (.loader_param))
	} > BTCM_PARAM AT > SERIAL_FLASH

	.loader_text :
	{
		KEEP(* (.loader_entry))
		* (.text)
		* (.text.*)
		* (.rodata)
		* (.rodata.*)
		* (.data)
		* (.data.*)
		. = ALIGN(4);
	} > BTCM_LOADER AT > SERIAL_FLASH

	.bss (NOLOAD) :
	{
		* (.bss)
		* (.bss.*)
		* (COMMON)
	} > BTCM_LOADER

	/DISCARD/ :
	{
		*(.ARM.exidx*)
		*(.ARM.extab*)
		*(.eh_frame*)
	}

	ASSERT(SIZEOF(.loader_param) == 0x4C,
		"the parameters for the loader must be 0x4C bytes: the boot ROM reads a fixed-size table")
	ASSERT(LOADADDR(.loader_param) == ORIGIN(SERIAL_FLASH),
		"the parameters for the loader must be the first bytes of the serial flash")
	ASSERT(LOADADDR(.loader_text) == ORIGIN(SERIAL_FLASH) + 0x4C,
		"the loader must follow its parameters: LDR_ADDR_NML points there")
	ASSERT(ADDR(.loader_text) == ORIGIN(BTCM_LOADER),
		"the loader must run at DEST_ADDR_NML")
	ASSERT(SIZEOF(.bss) == 0,
		"the loader must not own zero-initialized statics: nothing clears them")
}
