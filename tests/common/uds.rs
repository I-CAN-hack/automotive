use automotive::protocols::uds::{
    DTCFormatIdentifier, Error as UDSError, NegativeResponseCode, RoutineControlType, UDSClient,
};
use automotive::{Error, Transport};
use std::time::Duration;

/// The same service exchanges exercise ISO-TP and DoIP against Scapy.
pub async fn exercise_services<T: Transport>(uds: &UDSClient<'_, T>) {
    uds.tester_present().await.unwrap();

    let resp = uds.diagnostic_session_control(0x2).await;
    let security_access_denied =
        UDSError::NegativeResponse(NegativeResponseCode::SecurityAccessDenied);
    assert!(matches!(resp, Err(Error::UDSError(e)) if e == security_access_denied));

    let timing = uds.diagnostic_session_control(3).await.unwrap().unwrap();
    assert_eq!(timing.p2_server_max, Duration::from_secs(1));
    assert_eq!(timing.p2_star_server_max, Duration::from_secs(5));

    assert_eq!(uds.ecu_reset(1).await.unwrap(), None);
    assert_eq!(uds.ecu_reset(4).await.unwrap(), Some(2));

    assert_eq!(
        uds.security_access(1, None).await.unwrap(),
        Some(b"seed".to_vec())
    );
    let wrong_key = uds.security_access(2, Some(b"bad!")).await;
    assert!(matches!(
        wrong_key,
        Err(Error::UDSError(UDSError::NegativeResponse(
            NegativeResponseCode::InvalidKey
        )))
    ));
    assert_eq!(uds.security_access(2, Some(b"key!")).await.unwrap(), None);

    assert_eq!(
        uds.read_data_by_identifier(0x1234).await.unwrap(),
        b"deadbeef"
    );
    uds.write_data_by_identifier(0x1234, b"updated!")
        .await
        .unwrap();
    assert_eq!(
        uds.read_data_by_identifier(0x1234).await.unwrap(),
        b"updated!"
    );

    let dtcs = uds
        .read_dtc_information_number_of_dtc_by_status_mask(0xff)
        .await
        .unwrap();
    assert_eq!(dtcs.dtc_status_availability_mask, 0xff);
    assert_eq!(
        dtcs.dtc_format_identifier,
        DTCFormatIdentifier::ISO_14229_1_DTCFormat
    );
    assert_eq!(dtcs.dtc_count, 2);

    let address = 0x123456;
    let size = 8;
    assert_eq!(
        uds.read_memory_by_address_alfid(address, size, 3, 2)
            .await
            .unwrap(),
        b"abcdefgh"
    );
    uds.write_memory_by_address_alfid(address, size, 3, 2, b"written!")
        .await
        .unwrap();
    assert_eq!(
        uds.read_memory_by_address_alfid(address, size, 3, 2)
            .await
            .unwrap(),
        b"written!"
    );

    assert_eq!(
        uds.routine_control(RoutineControlType::Start, 0x1234, None)
            .await
            .unwrap(),
        Some(vec![0])
    );
    assert_eq!(
        uds.routine_control(RoutineControlType::Stop, 0x1234, None)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        uds.routine_control(RoutineControlType::RequestResults, 0x1234, None)
            .await
            .unwrap(),
        Some(b"done".to_vec())
    );

    assert_eq!(
        uds.request_download_alfid(0, 0, address, size, 3, 2)
            .await
            .unwrap(),
        10
    );
    assert_eq!(uds.transfer_data(1, Some(b"download")).await.unwrap(), None);
    assert_eq!(uds.request_transfer_exit(None).await.unwrap(), None);
    assert_eq!(
        uds.read_memory_by_address_alfid(address, size, 3, 2)
            .await
            .unwrap(),
        b"download"
    );

    assert_eq!(
        uds.request_upload_alfid(0, 0, address, size, 3, 2)
            .await
            .unwrap(),
        10
    );
    assert_eq!(
        uds.transfer_data(1, None).await.unwrap(),
        Some(b"download".to_vec())
    );
    assert_eq!(
        uds.request_transfer_exit(Some(&[0xaa, 0x55]))
            .await
            .unwrap(),
        Some(vec![0xaa, 0x55])
    );
}
