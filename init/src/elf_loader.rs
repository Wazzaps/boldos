use alloc::vec;
use alloc::{boxed::Box, format, vec::Vec};
use core::error::Error;
use core::mem::size_of;
use kernel_api::{
    user::{mm_create, region_create_virtual, region_write_full, Handle},
    RegionArg, RegionMapFlags,
};
use zerocopy::{FromBytes, Immutable, KnownLayout};

pub struct Elf {
    pub mm: Handle,
    pub entry_point: usize,
}

const PT_LOAD: u32 = 1;
const PF_W: u32 = 1 << 1;

#[repr(C)]
#[derive(Debug, FromBytes, KnownLayout, Immutable)]
pub struct Elf64Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[derive(Debug, FromBytes, KnownLayout, Immutable)]
#[repr(C)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

impl Elf {
    pub fn load(elf: &[u8]) -> Result<Self, Box<dyn Error>> {
        let ehdr = Elf64Ehdr::ref_from_bytes(&elf[..size_of::<Elf64Ehdr>()]).unwrap();
        if ehdr.e_ident
            != [
                0x7f, 0x45, 0x4c, 0x46, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00,
            ]
        {
            return Err("Invalid ELF magic".into());
        }
        if ehdr.e_type != 2 {
            return Err("ELF is not an executable".into());
        }
        if ehdr.e_machine != 0xb7 {
            return Err("ELF is not for AArch64".into());
        }
        if ehdr.e_version != 1 {
            return Err("ELF version is not supported".into());
        }
        if ehdr.e_phentsize != size_of::<Elf64Phdr>() as u16 {
            return Err("ELF program header size is not supported".into());
        }
        // println!("Loading ELF: {ehdr:#x?}");

        let mut regions: Vec<RegionArg> = vec![];
        let mut region_handles = vec![];
        let mut last_vaddr_end = 0;
        let mut last_flags = 0;
        for phdr_idx in 0..ehdr.e_phnum {
            let offset = (ehdr.e_phoff + phdr_idx as u64 * ehdr.e_phentsize as u64) as usize;
            let phdr =
                Elf64Phdr::ref_from_bytes(&elf[offset..offset + size_of::<Elf64Phdr>()]).unwrap();

            if phdr.p_type != PT_LOAD {
                continue;
            }
            // println!("Program header {phdr_idx}: {phdr:#x?}");
            let segment_pgal_start = (phdr.p_vaddr & !(4096 - 1)) as usize;
            // let segment_pgal_end = ((phdr.p_vaddr + phdr.p_memsz) as usize).next_multiple_of(4096);

            // Validations
            if !phdr.p_align.is_power_of_two() {
                return Err(format!(
                    "Segment {phdr_idx}: alignment ({:#x}) is not a power of two",
                    phdr.p_align
                )
                .into());
            }
            if phdr.p_align > 1 && phdr.p_vaddr % phdr.p_align != phdr.p_offset % phdr.p_align {
                return Err(format!(
                    "Segment {phdr_idx}: virtual address ({:#x}) is not aligned to file offset ({:#x})",
                    phdr.p_vaddr, phdr.p_offset
                )
                .into());
            }
            if phdr.p_vaddr < last_vaddr_end {
                return Err(format!(
                    "Segment {phdr_idx}: virtual address ({:#x}) is not greater than last segment end ({:#x})",
                    phdr.p_vaddr, last_vaddr_end
                )
                .into());
            }
            let mut effective_pgal_start = segment_pgal_start;
            let mut effective_pgal_size = (phdr.p_vaddr as usize + phdr.p_memsz as usize)
                .next_multiple_of(4096)
                - effective_pgal_start;
            if segment_pgal_start < last_vaddr_end as usize {
                if phdr.p_filesz != 0 {
                    return Err(format!(
                        "Segment {phdr_idx}: non-zeroed segment overlaps with previous segment when page-aligned"
                    )
                    .into());
                } else if phdr.p_flags != last_flags {
                    return Err(format!(
                        "Segment {phdr_idx}: differing flags overlapping with previous segment when page-aligned"
                    )
                    .into());
                } else {
                    effective_pgal_start += 4096;
                    effective_pgal_size -= 4096;
                }
            }
            last_vaddr_end = phdr.p_vaddr + phdr.p_memsz;
            last_flags = phdr.p_flags;

            if effective_pgal_size == 0 {
                continue;
            }

            let region = region_create_virtual(0, effective_pgal_size)?;
            if phdr.p_filesz != 0 {
                let slice =
                    &elf[phdr.p_offset as usize..phdr.p_offset as usize + phdr.p_filesz as usize];
                region_write_full(&region, slice, 0)?;
            }
            regions.push(RegionArg {
                region: region.as_u64(),
                size: effective_pgal_size,
                offset: 0,
                addr: effective_pgal_start,
                flags: if phdr.p_flags & PF_W != 0 {
                    RegionMapFlags::ReadWrite.bits()
                } else {
                    0
                },
                _padding: 0,
            });
            region_handles.push(region);
        }

        Ok(Self {
            mm: mm_create(&mut regions)?,
            entry_point: ehdr.e_entry as usize,
        })
    }
}
