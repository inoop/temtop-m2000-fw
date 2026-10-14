/* Flash execution for STM32F103VET6 */
/* App region after ARMILA bootloader (0x08000000-0x08005000) */

MEMORY
{
  FLASH : ORIGIN = 0x08005000, LENGTH = 492K
  RAM   : ORIGIN = 0x20000000, LENGTH = 64K
}

_stack_start = ORIGIN(RAM) + LENGTH(RAM);
