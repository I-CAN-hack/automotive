//! Constants for the UDS Client.
use strum_macros::EnumIter;

pub static POSITIVE_RESPONSE: u8 = 0x40;
pub static NEGATIVE_RESPONSE: u8 = 0x7f;
pub static ZERO_SUB_FUNCTION: u8 = 0x00;

/// Service Identifiers (SIDs) as defined in ISO 14229
#[derive(Debug, PartialEq, Copy, Clone, EnumIter)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum ServiceIdentifier {
    // Diagnostic and Communication Management
    DiagnosticSessionControl = 0x10,
    EcuReset = 0x11,
    SecurityAccess = 0x27,
    CommunicationControl = 0x28,
    TesterPresent = 0x3e,
    AccessTimingParameter = 0x83,
    SecuredDataTransmission = 0x84,
    ControlDTCSetting = 0x85,
    ResponseOnEvent = 0x86,
    LinkControl = 0x87,

    // Data Transmission
    ReadDataByIdentifier = 0x22,
    ReadMemoryByAddress = 0x23,
    ReadScalingDataByIdentifier = 0x24,
    ReadDataByPeriodicIdentifier = 0x2a,
    DynamicallyDefineDataIdentifier = 0x2c,
    WriteDataByIdentifier = 0x2e,
    WriteMemoryByAddress = 0x3d,

    // Stored Data Transmission
    ClearDiagnosticInformation = 0x14,
    ReadDTCInformation = 0x19,

    // Input/Output Control
    InputOutputControlByIdentifier = 0x2f,

    // Routine
    RoutineControl = 0x31,

    // Upload/Download
    RequestDownload = 0x34,
    RequestUpload = 0x35,
    TransferData = 0x36,
    RequestTransferExit = 0x37,
    RequestFileTransfer = 0x38,
}

/// Standard Data Identifiers (DIDs) as defined in ISO 14229
#[derive(Debug, PartialEq, Copy, Clone, EnumIter)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u16)]
pub enum DataIdentifier {
    BootSoftwareIdentification = 0xf180,
    ApplicationSoftwareIdentification = 0xf181,
    ApplicationDataIdentification = 0xf182,
    BootSoftwareFingerprint = 0xf183,
    ApplicationSoftwareFingerprint = 0xf184,
    ApplicationDataFingerprint = 0xf185,
    ActiveDiagnosticSession = 0xf186,
    VehicleManufacturerSparePartNumber = 0xf187,
    VehicleManufacturerEcuSoftwareNumber = 0xf188,
    VehicleManufacturerEcuSoftwareversionNumber = 0xf189,
    SystemSupplierIdentifier = 0xf18a,
    EcumanufacturingDate = 0xf18b,
    EcuSerialNumber = 0xf18c,
    SupportedFunctionalUnits = 0xf18d,
    VehicleManufacturerKitAssemblyPartNumber = 0xf18e,
    Vin = 0xf190,
    VehicleManufacturerEcuHardwareNumber = 0xf191,
    SystemSupplierEcuHardwareNumber = 0xf192,
    SystemSupplierEcuHardwareVersionNumber = 0xf193,
    SystemSupplierEcuSoftwareNumber = 0xf194,
    SystemSupplierEcuSoftwareVersionNumber = 0xf195,
    ExhaustRegulationOrTypeApprovalNumber = 0xf196,
    SystemNameOrEngineType = 0xf197,
    RepairShopCodeorTesterSerialNumber = 0xf198,
    ProgrammingDate = 0xf199,
    CalibrationRepairShopCodeOrCalibrationEquipmentSerialNumber = 0xf19a,
    CalibrationDate = 0xf19b,
    CalibrationEquipmentSoftwareNumber = 0xf19c,
    EcuInstallationDate = 0xf19d,
    OdxFile = 0xf19e,
    Entity = 0xf19f,
}

/// Diagnostic Session Type Sub-Function ID as defined in ISO 14229
#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum SessionType {
    Default = 0x01,
    Programming = 0x02,
    ExtendedDiagnostic = 0x03,
    SafetySystemDiagnostic = 0x04,
}

/// ECU Reset Sub-Function ID as defined in ISO 14229
#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum ResetType {
    /// Simulates power-on/start-up sequence after a power loss
    HardReset = 0x01,
    /// Simulates the driver turning the key off and then on again
    KeyOffOnReset = 0x02,
    /// Restart the application software
    SoftRest = 0x03,
    /// Applies to ECUs that are permanently powered. Forces the ECU to enter sleep mode.
    EnableRapidPowerShutDown = 0x04,
}

/// Security Access Type Sub-Function ID as defined in ISO 14229
#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum SecurityAccessType {
    RequestSeed = 0x01,
    SendKey = 0x02,
}

/// Routine Control Sub-Function ID as defined in ISO 14229
#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum RoutineControlType {
    Start = 0x01,
    Stop = 0x02,
    RequestResults = 0x03,
}

/// Read DTC Information Sub-Function ID as defined in ISO 14229
#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum ReportType {
    ReportNumberOfDTCByStatusMask = 0x01,
    ReportDTCByStatusMask = 0x02,
    ReportDTCSnapshotIdentification = 0x03,
    ReportDTCSnapshotRecordByDTCNumber = 0x04,
    ReportDTCStoredDataByRecordNumber = 0x05,
    ReportDTCExtDataRecordByDTCNumber = 0x06,
    ReportNumberOfDTCBySeverityMaskRecord = 0x07,
    ReportDTCBySeverityMaskRecord = 0x08,
    ReportSeverityInformationOfDTC = 0x09,
    ReportSupportedDTC = 0x0A,
    ReportFirstTestFailedDTC = 0x0B,
    ReportFirstConfirmedDTC = 0x0C,
    ReportMostRecentTestFailedDTC = 0x0D,
    ReportMostRecentConfirmedDTC = 0x0F,
    ReportMirrorMemoryDTCByStatusMask = 0x10,
    ReportMirrorMemoryDTCExtDataRecordByDTCNumber = 0x11,
    ReportNumberOfMirrorMemoryDTCByStatusMask = 0x12,
    ReportNumberOfEmissionsOBDDTCByStatusMask = 0x13,
    ReportEmissionsOBDDTCByStatusMask = 0x14,
    ReportDTCFaultDetectionCounter = 0x15,
    ReportDTCWithPermanentStatus = 0x16,
    ReportDTCExtDataRecordByRecordNumber = 0x17,
    ReportUserDefMemoryDTCByStatusMask = 0x18,
    ReportUserDefMemoryDTCSnapshotRecordByDTCNumber = 0x19,
    ReportUserDefMemoryDTCExtDataRecordByDTCNumber = 0x42,
    ReportWWHOBDDTCWithPermanentStatus = 0x55,
}
