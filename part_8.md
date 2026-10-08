# THE PROPELLER KERNEL - PART 8

## Introduction

Let's take a slight interlude for a moment. In Part 6, you might have been able to reference the Propeller code, or wrote your own code, to set up and enable the MMU on 32-bit ARM. When I originally wrote the 32-bit ARM MMU setup code for Propeller, I made a *very* subtle mistake. In fact, I made the same mistake for AArch64 and fixed it at some point, but never considered the 32-bit code might be broken as well.

Fast forward several months, and I was mired in data aborts while trying to boot the secondary cores.

## The Symptoms

In Parts 6 and 7, we set up the initial translation tables. In a future part, we are going to duplicate Part 7 for the secondary cores. That will involve allocating memory for the secondary core stacks and writing the pointers to a table that the cores can reference when they start up.

From the programmer's view, the translation tables and the stack table are "in memory." However, caching exists and, if you followed along with Propeller's code, you enabled caching for the inner shareable domain.

With AArch64, the secondary cores had no issue booting. In fact, on a Cortex-A7 Raspberry Pi 2, the ARM build had no trouble booting its secondary cores. But, an ARM build running on a Cortex-A53 Raspberry Pi would data abort. I never did figure out why the Cortex-A7 worked, but it is likely it would not have worked reliably beyond early boot.

Examining memory from Core 1 on the Cortex-A53 using the JTAG debugger revealed that Core 1 was only seeing partial writes of the translation tables and stack table. So, right away, it was clear there was a caching problem. But, why? *Obviously* caching was enabled and ARM [cache coherency](https://support.arm.com/documentation/den0013/0400/Multi-core-processors/Cache-coherency) should give Core 1 the same view of memory as Core 0 even if the tables have not been fully written back to main memory. *Obviously*.

## Debugging

I started with the hypothesis that, because the secondary cores were idle with their MMUs and caches disabled, they were not participating in cache coherency. I ended up writing a quick function to fully evict a block of memory from the cache, then making sure I evict the translation tables and stack table before booting the secondary cores.

I really did not like this. It broke my mental model of cache coherency and made me worry about what it meant for cache maintenance in the future. However, it did work to some extent. The secondary cores now had the same view of memory as Core 0.

And...the cores would still data abort later when trying to acquire the spin lock protecting the serial debug output driver.

Looking closer, this new data abort had a [DFSR](https://arm.jonpalmisc.com/latest_sysreg/AArch32-dfsr) value 0x235 and a [DFAR](https://arm.jonpalmisc.com/latest_sysreg/AArch32-dfar) value that matched the location of the spin lock variable. With LPAE enabled, the lower six bits of 0x235 are 0b110101 which maps to the fault:

> IMPLEMENTATION DEFINED fault (Unsupported Exclusive access)

Um...ok. Well, I copied the spin lock code from the ARM reference manual. So...[What the French, Toast?](https://www.youtube.com/watch?v=SoqWpp6jP-s).

## ChatGPT

Yeah, OK, I had a conversation with ChatGPT. Regular searches were not working. I *obviously* had caching configured, the pertinent information was visible to the secondary cores, and the spin lock code should be correct.

ChatGPT was actually as confused as I was even after giving it code snippets, but it did give me a handy list of assumptions to double-check.

## Obviously

I'm sure you've guessed by now. *OBVIOUSLY*, caching was not configured correctly. The `MAIR0` and `MAIR1` values were being [configured correctly](https://github.com/slakpi/Propeller-Kernel/blob/30e1bc6b50d23c68cce2760fcb01330357254d0f/src/arch/arm/start/mm.s#L50) for inner and outer write-back caching.

However! The function [init_mair](https://github.com/slakpi/Propeller-Kernel/blob/30e1bc6b50d23c68cce2760fcb01330357254d0f/src/arch/arm/start/mm.s#L599) was being called from [mmu_create_kernel_page_tables](https://github.com/slakpi/Propeller-Kernel/blob/30e1bc6b50d23c68cce2760fcb01330357254d0f/src/arch/arm/start/mm.s#L147). `mmu_create_kernel_page_tables` is only called by the primary core! The tables end up in shared physical memory, but `MAIR0` and `MAIR1` are local to each core. So, the secondary cores were not treating normal memory as cacheable despite caching being enabled.

`ldrex` and `strex` require cacheable memory, otherwise they raise...drum roll: a data abort with the code 0b110101.

## The Fix

Well, the fix was actually obvious enough because I had already caught and fixed it on the AArch64 side: set up `MAIR0` and `MAIR1` in [mmu_setup_and_enable](https://github.com/slakpi/Propeller-Kernel/blob/a2a036b12add6617c1a266e168b47f42d5f90126/src/arch/arm/start/mm.s#L428). That function is called by all of the cores and it is a much more logical place to do that setup work anyway.

## The Morals

I think I had all the information right in front of me. It should have been obvious I did not set up caching correctly rather than obvious that I had. But, the Cortex-A7 build seeming to work really threw me. Further, the idea that I would have to manually evict the data from the cache broke my fairly solid understanding of cache coherency and should have been another clue.

When working with less experienced software engineers that are stuck on a problem, I ask them to write down all of their assumptions on paper. Usually, just that act triggers a sudden realization of exactly what they were incorrectly assuming. If not right there, however, they usually find it after going down the list and double-checking.

I did not do that here. So, after ChatGPT gave me a list of things to check and I immediately realized what I did wrong, I wrote to it:

> Damn it. I'm an idiot.

ChatGPT wrote back:

> Nope. You're debugging AArch32 LPAE startup code. That's basically an invitation to spend an afternoon staring at MCRR instructions and wondering whether reality is still mapped.

So true, ChatGPT. So true.

-----
[Part 9](https://slakpi.github.io/Propeller-Kernel/part_9.html)

© Randy Widell