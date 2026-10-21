use automotive::can::{Frame, Identifier};

fn id() -> Identifier {
    Identifier::standard(0x123).unwrap()
}

#[test]
fn identifier_bounds_and_explicit_format() {
    assert!(Identifier::standard(0).is_ok());
    assert!(Identifier::standard(0x7ff).is_ok());
    assert!(Identifier::standard(0x800).is_err());
    assert!(Identifier::standard(u32::MAX).is_err());
    assert!(Identifier::extended(0).is_ok());
    assert!(Identifier::extended(0x1fff_ffff).is_ok());
    assert!(Identifier::extended(0x2000_0000).is_err());
    assert!(Identifier::extended(u32::MAX).is_err());
    assert!(Identifier::try_from(u32::MAX).is_err());
    assert!(Identifier::try_from(0x7ff).unwrap().is_standard());
    assert!(Identifier::try_from(0x800).unwrap().is_extended());
    assert!(Identifier::extended(0x123).unwrap().is_extended());
}

#[test]
fn classic_frames_reject_fd_flags() {
    assert!(Frame::builder(id()).brs(true).build().is_err());
    assert!(Frame::builder(id()).esi(true).build().is_err());
}

#[test]
fn validates_every_payload_length_without_padding() {
    // ISO 11898-1:2015, Table 5 (independent of the implementation's lookup).
    let fd_lengths = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];
    for len in 0..=65 {
        let data = vec![0xaa; len];
        let classic = Frame::builder(id()).data(&data).build();
        assert_eq!(classic.is_ok(), len <= 8, "classic length {len}");
        let fd = Frame::builder(id()).fd(true).data(&data).build();
        assert_eq!(fd.is_ok(), fd_lengths.contains(&len), "FD length {len}");
        if let Ok(frame) = fd {
            assert!(frame.is_fd());
            assert_eq!(frame.data(), data);
            assert_eq!(
                frame.dlc(),
                fd_lengths
                    .iter()
                    .position(|&n| n == len)
                    .map(|dlc| dlc as u8)
            );
        }
    }
}

#[test]
fn remote_frames_reject_out_of_range_dlc() {
    assert!(Frame::remote(0, id(), 15).is_ok());
    assert!(Frame::remote(0, id(), 16).is_err());
    assert!(Frame::remote(0, id(), u8::MAX).is_err());
}
