"""Static declarations for CCP enums generated from Rust."""

from enum import IntEnum, IntFlag

class Command(IntEnum):
    CONNECT = 0x01
    SET_MTA = 0x02
    DOWNLOAD = 0x03
    UPLOAD = 0x04
    DISCONNECT = 0x07
    SHORT_UPLOAD = 0x0F
    GET_SEED = 0x12
    UNLOCK = 0x13
    EXCHANGE_ID = 0x17
    GET_CCP_VERSION = 0x1B
    DOWNLOAD6 = 0x23

class Resource(IntFlag):
    CAL = 0x01
    DAQ = 0x02
    PGM = 0x40

class ErrorCode(IntEnum):
    ACKNOWLEDGE = 0x00
    DAQ_OVERLOAD = 0x01
    COMMAND_BUSY = 0x10
    DAQ_BUSY = 0x11
    INTERNAL_TIMEOUT = 0x12
    KEY_REQUEST = 0x18
    SESSION_STATUS_REQUEST = 0x19
    COLD_START_REQUEST = 0x20
    CALIBRATION_DATA_INIT_REQUEST = 0x21
    DAQ_LIST_INIT_REQUEST = 0x22
    CODE_UPDATE_REQUEST = 0x23
    COMMAND_UNKNOWN = 0x30
    COMMAND_SYNTAX = 0x31
    OUT_OF_RANGE = 0x32
    ACCESS_DENIED = 0x33
    OVERLOAD = 0x34
    ACCESS_LOCKED = 0x35
    RESOURCE_NOT_AVAILABLE = 0x36
