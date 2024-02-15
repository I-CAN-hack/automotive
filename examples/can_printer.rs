use automotive::panda::Panda;


fn main() {
    let panda = Panda::new().unwrap();
    println!("{:?}", panda.get_hw_type().unwrap());
}
