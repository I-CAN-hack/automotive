use socketcan::EmbeddedFrame;

impl From<socketcan::frame::CanAnyFrame> for crate::can::Frame {
    fn from(frame: socketcan::frame::CanAnyFrame) -> Self {
        match frame {
            socketcan::frame::CanAnyFrame::Normal(data) => data.into(),
            socketcan::frame::CanAnyFrame::Fd(fd) => fd.into(),
            _ => unimplemented!(),
        }
    }
}

impl From<socketcan::frame::CanDataFrame> for crate::can::Frame {
    fn from(frame: socketcan::frame::CanDataFrame) -> crate::can::Frame {
        crate::can::Frame {
            bus: 0,
            id: frame.id().into(),
            data: frame.data().to_vec(),
            loopback: false,
            fd: false,
        }
    }
}

impl From<socketcan::frame::CanFdFrame> for crate::can::Frame {
    fn from(frame: socketcan::frame::CanFdFrame) -> crate::can::Frame {
        crate::can::Frame {
            bus: 0,
            id: frame.id().into(),
            data: frame.data().to_vec(),
            loopback: false,
            fd: true,
        }
    }
}

impl From<crate::can::Frame> for socketcan::frame::CanAnyFrame {
    fn from(frame: crate::can::Frame) -> Self {
        let id: socketcan::Id = frame.id.into();
        match frame.fd {
            true => socketcan::frame::CanAnyFrame::Fd(
                socketcan::frame::CanFdFrame::new(id, &frame.data).unwrap(),
            ),
            false => socketcan::frame::CanAnyFrame::Normal(
                socketcan::frame::CanDataFrame::new(id, &frame.data).unwrap(),
            ),
        }
    }
}

impl From<socketcan::Id> for crate::can::Identifier {
    fn from(id: socketcan::Id) -> Self {
        match id {
            socketcan::Id::Standard(id) => crate::can::Identifier::Standard(id.as_raw() as u32),
            socketcan::Id::Extended(id) => crate::can::Identifier::Extended(id.as_raw()),
        }
    }
}

impl From<crate::can::Identifier> for socketcan::Id {
    fn from(id: crate::can::Identifier) -> Self {
        match id {
            crate::can::Identifier::Standard(id) => {
                socketcan::Id::Standard(socketcan::StandardId::new(id as u16).unwrap())
            }
            crate::can::Identifier::Extended(id) => {
                socketcan::Id::Extended(socketcan::ExtendedId::new(id).unwrap())
            }
        }
    }
}
