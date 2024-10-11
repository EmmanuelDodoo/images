mod jpeg;
use jpeg::JPEGHeader;

fn main() {
    let image = "cat.jpg";

    let stream = std::fs::read(image).unwrap();

    match JPEGHeader::new(stream) {
        Ok(_jpeg_header) => {
            println!("Done reading!");
        }
        Err(err) => {
            println!("{}", err)
        }
    }
}
