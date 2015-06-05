use std::io::prelude::*;
use std::io;
use std::path::Path;
use std::fs;
use std::fmt;
use byteorder;
use byteorder::ReadBytesExt;
use elf;
use std::collections::HashMap;

macro_rules! read_u64 {
    ($data:ident, $io:ident) => (
        match $data {
            elf::ELFDATA2LSB => { $io.read_u64::<byteorder::LittleEndian>() },
            elf::ELFDATA2MSB => { $io.read_u64::<byteorder::BigEndian>()},
            _ => { return Err(io::Error::new(io::ErrorKind::Other, "invalid endianness")) },
        }
    );
}

macro_rules! read_u32 {
    ($data:ident, $io:ident) => (
        match $data {
            elf::ELFDATA2LSB => { $io.read_u32::<byteorder::LittleEndian>() },
            elf::ELFDATA2MSB => { $io.read_u32::<byteorder::BigEndian>()},
            _ => { return Err(io::Error::new(io::ErrorKind::Other, "invalid endianness")) },
        }
    );
}

macro_rules! read_u16 {
    ($data:ident, $io:ident) => (
        match $data {
            elf::ELFDATA2LSB => { $io.read_u16::<byteorder::LittleEndian>() },
            elf::ELFDATA2MSB => { $io.read_u16::<byteorder::BigEndian>()},
            _ => { return Err(io::Error::new(io::ErrorKind::Other, "invalid endianness")) },
        }
    );
}

fn get_elf_string(data: &Vec<u8>, start: usize) -> String {
    let mut end = 0usize;
    for i in start..data.len() {
        if data[i] == 0u8 {
            end = i;
            break;
        }
    }

    let mut ret = String::with_capacity(end - start);
    for i in start..end {
        ret.push(data[i] as char);
    }

    ret
}

pub struct File {
    hdr: elf::FileHeader,
    sections: HashMap<String, Section>,
}

pub struct Section {
    hdr: elf::SectionHeader,
    data: Vec<u8>,
}

impl File {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<File, io::Error> {
        let rf = try!(fs::File::open(path));
        let mut f = io::BufReader::new(rf);

        let mut eident = [0u8; elf::EI_NIDENT];
        try!(f.read(&mut eident));

        if eident[0..4] != elf::ELFMAG {
            return Err(io::Error::new(io::ErrorKind::Other, "invalid magic number"));
        }

        let class = elf::Class(eident[elf::EI_CLASS]);
        let data = elf::Data(eident[elf::EI_DATA]);
        let os_abi = elf::OsAbi(eident[elf::EI_OSABI]);
        let abi_version = eident[elf::EI_ABIVERSION];

        let elf_type = elf::Type(try!(read_u16!(data, f)));
        let machine = elf::Machine(try!(read_u16!(data, f)));
        let version = elf::Version(try!(read_u32!(data, f)));

        let mut entry: u64;
        let mut phoff: u64;
        let mut shoff: u64;

        match class {
            elf::ELFCLASS32 => {
                entry = try!(read_u32!(data, f)) as u64;
                phoff = try!(read_u32!(data, f)) as u64;
                shoff = try!(read_u32!(data, f)) as u64;
            }
            elf::ELFCLASS64 => {
                entry = try!(read_u64!(data, f));
                phoff = try!(read_u64!(data, f));
                shoff = try!(read_u64!(data, f));
            }
            _ => return Err(io::Error::new(io::ErrorKind::Other, "invalid class")),
        }

        let flags = try!(read_u32!(data, f));
        let ehsize = try!(read_u16!(data, f));
        let phentsize = try!(read_u16!(data, f));
        let phnum = try!(read_u16!(data, f));
        let shentsize = try!(read_u16!(data, f));
        let shnum = try!(read_u16!(data, f));
        let shstrndx = try!(read_u16!(data, f));

        let mut sections = HashMap::new();
        let mut sections_lst = Vec::new();
        let mut sections_data = Vec::new();

        let mut name_idxs = Vec::new();
        try!(f.seek(io::SeekFrom::Start(shoff)));

        for _ in 0..shnum {
            let name = String::new();
            let mut shtype: elf::SectionType;
            let mut flags: elf::SectionFlag;
            let mut addr: u64;
            let mut offset: u64;
            let mut size: u64;
            let mut link: u32;
            let mut info: u32;
            let mut addralign: u64;
            let mut entsize: u64;

            name_idxs.push(try!(read_u32!(data, f)));
            shtype = elf::SectionType(try!(read_u32!(data, f)));
            match class {
                elf::ELFCLASS32 => {
                    flags = elf::SectionFlag(try!(read_u32!(data, f)) as u64);
                    addr = try!(read_u32!(data, f)) as u64;
                    offset = try!(read_u32!(data, f)) as u64;
                    size = try!(read_u32!(data, f)) as u64;
                    link = try!(read_u32!(data, f));
                    info = try!(read_u32!(data, f));
                    addralign = try!(read_u32!(data, f)) as u64;
                    entsize = try!(read_u32!(data, f)) as u64;
                }
                elf::ELFCLASS64 => {
                    flags = elf::SectionFlag(try!(read_u64!(data, f)));
                    addr = try!(read_u64!(data, f));
                    offset = try!(read_u64!(data, f));
                    size = try!(read_u64!(data, f));
                    link = try!(read_u32!(data, f));
                    info = try!(read_u32!(data, f));
                    addralign = try!(read_u64!(data, f));
                    entsize = try!(read_u64!(data, f));
                }
                _ => unreachable!(),
            }

            sections_lst.push(elf::SectionHeader {
                name: name,
                shtype: shtype,
                flags: flags,
                addr: addr,
                offset: offset,
                size: size,
                link: link,
                info: info,
                addralign: addralign,
                entsize: entsize,
            });
        }

        for i in 0..shnum {
            let off = sections_lst[i as usize].offset;
            let size = sections_lst[i as usize].size;
            try!(f.seek(io::SeekFrom::Start(off)));
            let data: Vec<u8> = io::Read::by_ref(&mut f).bytes().map(|x| x.unwrap()).take(size as usize).collect();
            sections_data.push(data);
        }

        for i in 0..shnum {
            sections_lst[i as usize].name = get_elf_string(&sections_data[shstrndx as usize], name_idxs[i as usize] as usize);
        }

        for (hdr, data) in sections_lst.into_iter().zip(sections_data.into_iter()) {
            sections.insert(hdr.name.clone(), Section { hdr: hdr, data: data });
        }

        Ok(File {
            hdr: elf::FileHeader {
                class: class,
                data: data,
                version: version,
                os_abi: os_abi,
                abi_version: abi_version,
                elf_type: elf_type,
                machine: machine,
                entrypoint: entry,
            },
            sections: sections,
        })
    }

    pub fn sections(&self) -> &HashMap<String, Section> {
        &self.sections
    }
}

impl Section {
    pub fn header(&self) -> &elf::SectionHeader {
        &self.hdr
    }
    pub fn data(&self) -> &Vec<u8> {
        &self.data
    }
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ELF file version {:x}", self.hdr.version.0)
    }
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ELF section '{}' from {:#x} to {:#x}", self.hdr.name, self.hdr.addr, self.hdr.addr + self.hdr.size)
    }
}
