use std::io::prelude::*;
use std::io;
use std::path::Path;
use std::fs;
use std::fmt;
use byteorder;
use byteorder::ReadBytesExt;
use elf::types;
use std::collections::HashMap;
use std::collections::hash_map;

macro_rules! read_u64 {
    ($data:ident, $io:ident) => (
        match $data {
            types::ELFDATA2LSB => { $io.read_u64::<byteorder::LittleEndian>() },
            types::ELFDATA2MSB => { $io.read_u64::<byteorder::BigEndian>()},
            _ => { return Err(io::Error::new(io::ErrorKind::Other, "invalid endianness")) },
        }
    );
}

macro_rules! read_u32 {
    ($data:ident, $io:ident) => (
        match $data {
            types::ELFDATA2LSB => { $io.read_u32::<byteorder::LittleEndian>() },
            types::ELFDATA2MSB => { $io.read_u32::<byteorder::BigEndian>()},
            _ => { return Err(io::Error::new(io::ErrorKind::Other, "invalid endianness")) },
        }
    );
}

macro_rules! read_u16 {
    ($data:ident, $io:ident) => (
        match $data {
            types::ELFDATA2LSB => { $io.read_u16::<byteorder::LittleEndian>() },
            types::ELFDATA2MSB => { $io.read_u16::<byteorder::BigEndian>()},
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
    hdr: types::FileHeader,
    sections: HashMap<String, Section>,
}

pub struct Section {
    hdr: types::SectionHeader,
    data: Vec<u8>,
}

impl File {
    pub fn parse<R: io::Read + io::Seek>(r: &mut R) -> Result<File, io::Error> {
        r.seek(io::SeekFrom::Start(0));
        let mut eident = [0u8; types::EI_NIDENT];
        try!(r.read(&mut eident));

        if eident[0..4] != types::ELFMAG {
            return Err(io::Error::new(io::ErrorKind::Other, "invalid magic number"));
        }

        let class = types::Class(eident[types::EI_CLASS]);
        let data = types::Data(eident[types::EI_DATA]);
        let os_abi = types::OsAbi(eident[types::EI_OSABI]);
        let abi_version = eident[types::EI_ABIVERSION];

        let elf_type = types::Type(try!(read_u16!(data, r)));
        let machine = types::Machine(try!(read_u16!(data, r)));
        let version = types::Version(try!(read_u32!(data, r)));

        let mut entry: u64;
        let mut phoff: u64;
        let mut shoff: u64;

        match class {
            types::ELFCLASS32 => {
                entry = try!(read_u32!(data, r)) as u64;
                phoff = try!(read_u32!(data, r)) as u64;
                shoff = try!(read_u32!(data, r)) as u64;
            }
            types::ELFCLASS64 => {
                entry = try!(read_u64!(data, r));
                phoff = try!(read_u64!(data, r));
                shoff = try!(read_u64!(data, r));
            }
            _ => return Err(io::Error::new(io::ErrorKind::Other, "invalid class")),
        }

        let flags = try!(read_u32!(data, r));
        let ehsize = try!(read_u16!(data, r));
        let phentsize = try!(read_u16!(data, r));
        let phnum = try!(read_u16!(data, r));
        let shentsize = try!(read_u16!(data, r));
        let shnum = try!(read_u16!(data, r));
        let shstrndx = try!(read_u16!(data, r));

        let mut sections = HashMap::new();
        let mut sections_lst = Vec::new();
        let mut sections_data = Vec::new();

        let mut name_idxs = Vec::new();
        try!(r.seek(io::SeekFrom::Start(shoff)));

        for _ in 0..shnum {
            let name = String::new();
            let mut shtype: types::SectionType;
            let mut flags: types::SectionFlag;
            let mut addr: u64;
            let mut offset: u64;
            let mut size: u64;
            let mut link: u32;
            let mut info: u32;
            let mut addralign: u64;
            let mut entsize: u64;

            name_idxs.push(try!(read_u32!(data, r)));
            shtype = types::SectionType(try!(read_u32!(data, r)));
            match class {
                types::ELFCLASS32 => {
                    flags = types::SectionFlag(try!(read_u32!(data, r)) as u64);
                    addr = try!(read_u32!(data, r)) as u64;
                    offset = try!(read_u32!(data, r)) as u64;
                    size = try!(read_u32!(data, r)) as u64;
                    link = try!(read_u32!(data, r));
                    info = try!(read_u32!(data, r));
                    addralign = try!(read_u32!(data, r)) as u64;
                    entsize = try!(read_u32!(data, r)) as u64;
                }
                types::ELFCLASS64 => {
                    flags = types::SectionFlag(try!(read_u64!(data, r)));
                    addr = try!(read_u64!(data, r));
                    offset = try!(read_u64!(data, r));
                    size = try!(read_u64!(data, r));
                    link = try!(read_u32!(data, r));
                    info = try!(read_u32!(data, r));
                    addralign = try!(read_u64!(data, r));
                    entsize = try!(read_u64!(data, r));
                }
                _ => unreachable!(),
            }

            sections_lst.push(types::SectionHeader {
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
            try!(r.seek(io::SeekFrom::Start(off)));
            let data: Vec<u8> = io::Read::by_ref(r).bytes().map(|x| x.unwrap()).take(size as usize).collect();
            sections_data.push(data);
        }

        for i in 0..shnum {
            sections_lst[i as usize].name = get_elf_string(&sections_data[shstrndx as usize], name_idxs[i as usize] as usize);
        }

        for (hdr, data) in sections_lst.into_iter().zip(sections_data.into_iter()) {
            sections.insert(hdr.name.clone(), Section { hdr: hdr, data: data });
        }

        Ok(File {
            hdr: types::FileHeader {
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

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(writeln!(f, "ELF file"));
        try!(write!(f, "{}", self.hdr));
        try!(writeln!(f, "ELF sections"));
        for section in self.sections.values() {
            try!(write!(f, "{}", section));
        }
        Ok(())
    }
}

impl ::Object for File {
    fn arch(&self) -> ::Arch {
        ::Arch::Unknown
    }
    fn get_section(&self, name: &str) -> Option<::Section> {
        if let Some(sect) = self.sections.get(name) {
            Some(::Section {
                name: sect.hdr.name.clone(),
                addr: sect.hdr.addr,
                size: sect.hdr.size,
                data: sect.data.clone(), // FIXME don't clone data, store sections
            })
        } else {
            None
        }
    }
}

impl Section {
    pub fn header(&self) -> &types::SectionHeader {
        &self.hdr
    }
    pub fn data(&self) -> &Vec<u8> {
        &self.data
    }
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.hdr)
    }
}
