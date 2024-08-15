mod jpeg;
use jpeg::JPEG;

fn main() {
    let image = "test.jpg";

    let stream = std::fs::read(image).unwrap();

    match JPEG::new(stream) {
        Ok(jpeg) => {
            println!("{:?}", jpeg);
        }
        Err(err) => {
            println!("{}", err)
        }
    }
}
