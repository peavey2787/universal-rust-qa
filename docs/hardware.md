# Hardware / Embedded QA — Phase 16

`QA-HW-*` is profile-gated. It checks MMIO volatile access, interrupt stack budgets and forbidden ISR operations, DMA alignment contracts, configured embedded target builds, and optional linker-map evidence. Unsupported or unconfigured target evidence is reported as `N/A`/`UNKNOWN`, never silently passed.
