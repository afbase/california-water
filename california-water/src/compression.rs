use std::{io, io::{Write, Read, BufReader}, str};
use tar::Archive;
use lzma_rs::xz_decompress;
use csv::{Reader, StringRecordIter};
pub static TAR_OBJECT: &[u8] = include_bytes!("../obj/output.tar.lzma");

pub fn decompress_tar_file_to_csv_string(input: &[u8]) -> Vec<u8> {
    let mut tar_object_buffer = BufReader::new(input);
    let mut decompress_output: Vec<u8> = Vec::new();
    xz_decompress(&mut tar_object_buffer, &mut decompress_output).unwrap();
    // read decompress_output with archive
    let mut tar_file_from_decompress_output = Archive::new(decompress_output.as_slice());
    let tar_file_enumerator = tar_file_from_decompress_output.entries().unwrap().enumerate();
    let mut buf: Vec<u8> = Vec::new();
    for (_i, csv_file_result) in tar_file_enumerator {
        let mut csv_file = csv_file_result.unwrap();
        csv_file.read_to_end(&mut buf);
        break;
    }
    buf
}

#[cfg(test)]
mod test {
    use super::{decompress_tar_file_to_csv_string, TAR_OBJECT};
    use sha3::{Digest, Sha3_384};
    use hex_literal::hex;
    #[test]
    fn test_decompress_tar_file_to_csv_string() {
        let output = decompress_tar_file_to_csv_string(TAR_OBJECT);
        let mut hasher = Sha3_384::new();
        let bytes = output.as_slice();
        // let strs = std::str::from_utf8(bytes).unwrap();
        // println!("{:?}", strs);
        hasher.update(bytes);
        let result = hasher.finalize();
        assert_eq!(result[..], hex!("35f323d919c0c9ef3bd00f2421c28195506eb67cc971e7a9e3529742337ffdff3636ce839035fa273d90301245fff39d"));
    }
}