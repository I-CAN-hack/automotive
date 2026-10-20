use automotive::can::{
    ErrorFlag, ExtendedId, Frame, FrameContent, Identifier, StandardId, DLC_TO_LEN,
};

fn id() -> Identifier {
    Identifier::standard(0x123).unwrap()
}

#[test]
fn identifier_bounds_and_explicit_format() {
    assert!(StandardId::new(0).is_ok());
    assert!(StandardId::new(0x7ff).is_ok());
    assert!(StandardId::new(0x800).is_err());
    assert!(ExtendedId::new(0).is_ok());
    assert!(ExtendedId::new(0x1fff_ffff).is_ok());
    assert!(ExtendedId::new(0x2000_0000).is_err());
    assert!(Identifier::try_from(u32::MAX).is_err());
    assert!(Identifier::try_from(0x7ff).unwrap().is_standard());
    assert!(Identifier::try_from(0x800).unwrap().is_extended());
    let extended = Identifier::extended(0x123).unwrap();
    assert!(extended.is_extended());
    assert_eq!(extended.as_raw(), id().as_raw());
    assert_ne!(extended, id());
}

#[test]
fn validates_every_payload_length_without_padding() {
    // ISO 11898-1:2015, Table 5 (independent of the implementation's lookup).
    let fd_lengths = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];
    assert_eq!(DLC_TO_LEN, fd_lengths);
    for len in 0..=65 {
        let data = vec![0xaa; len];
        let classic = Frame::builder(id()).data(data.clone()).build();
        assert_eq!(classic.is_ok(), len <= 8, "classic length {len}");
        let fd = Frame::builder(id()).fd().data(data.clone()).build();
        assert_eq!(fd.is_ok(), fd_lengths.contains(&len), "FD length {len}");
        if let Ok(frame) = fd {
            assert_eq!(frame.data(), data);
            assert_eq!(DLC_TO_LEN[frame.dlc().unwrap() as usize], len);
        }
    }
}

#[test]
fn short_fd_frames_remain_fd_and_keep_flags() {
    for len in 0..=8 {
        let frame = Frame::builder(id())
            .bus(2)
            .loopback(true)
            .data(vec![0x55; len])
            .fd()
            .brs(true)
            .esi(true)
            .build()
            .unwrap();
        assert_eq!(frame.bus, 2);
        assert!(frame.loopback);
        assert!(frame.is_data());
        assert_eq!(frame.id(), Some(id()));
        let FrameContent::Fd(fd) = frame.content() else {
            panic!("lost FD format")
        };
        assert!(fd.brs());
        assert!(fd.esi());
        assert_eq!(fd.data().len(), len);
    }
}

#[test]
fn remote_frames_have_dlc_without_data() {
    for dlc in 0..=15 {
        let frame = Frame::remote_builder(id(), dlc).build().unwrap();
        assert!(frame.is_remote());
        assert!(!frame.is_fd());
        assert!(!frame.is_data());
        assert_eq!(frame.id(), Some(id()));
        assert_eq!(frame.dlc(), Some(dlc));
        assert!(frame.data().is_empty());
        let FrameContent::Remote(remote) = frame.content() else {
            unreachable!()
        };
        assert_eq!(remote.requested_len(), dlc.min(8) as usize);
    }
    assert!(Frame::remote_builder(id(), 16).build().is_err());
    assert!(Frame::remote_builder(id(), u8::MAX).build().is_err());
}

#[test]
fn error_frames_have_no_identifier_dlc_or_payload() {
    for flag in [ErrorFlag::Active, ErrorFlag::Passive] {
        let frame = Frame::error_builder(flag).bus(3).build().unwrap();
        assert_eq!(frame.bus, 3);
        assert_eq!(frame.content(), &FrameContent::Error(flag));
        assert!(frame.is_error());
        assert!(!frame.is_data());
        assert_eq!(frame.id(), None);
        assert_eq!(frame.dlc(), None);
        assert!(frame.data().is_empty());
    }
}

#[cfg(feature = "serde")]
#[test]
fn serialization_round_trips_each_frame_variant() {
    let frames = [
        Frame::builder(id()).data([0xaa; 8]).build().unwrap(),
        Frame::builder(Identifier::extended(0x123).unwrap())
            .fd()
            .brs(true)
            .esi(true)
            .data([0x55; 12])
            .build()
            .unwrap(),
        Frame::remote_builder(id(), 15).build().unwrap(),
        Frame::error_builder(ErrorFlag::Active).build().unwrap(),
        Frame::error_builder(ErrorFlag::Passive).build().unwrap(),
    ];
    for frame in frames {
        let serialized = serde_json::to_string(&frame).unwrap();
        assert_eq!(serde_json::from_str::<Frame>(&serialized).unwrap(), frame);
    }
}

#[cfg(feature = "serde")]
#[test]
fn deserialization_cannot_bypass_frame_invariants() {
    use serde_json::json;
    for content in [
        json!({"Classic": {"id": {"Standard": 0x800}, "data": []}}),
        json!({"Classic": {"id": {"Extended": 0x2000_0000}, "data": []}}),
        json!({"Classic": {"id": {"Standard": 1}, "data": vec![0; 9]}}),
        json!({"Fd": {"id": {"Standard": 1}, "data": vec![0; 9], "brs": false, "esi": false}}),
        json!({"Remote": {"id": {"Standard": 1}, "dlc": 16}}),
        json!({"Remote": {"id": {"Standard": 1}, "dlc": 8, "data": [1]}}),
        json!({"Classic": {"id": {"Standard": 1}, "data": [], "brs": true}}),
    ] {
        let value = json!({"bus": 0, "loopback": false, "content": content});
        assert!(
            serde_json::from_value::<Frame>(value.clone()).is_err(),
            "{value}"
        );
    }
}
