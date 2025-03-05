package metadata

remap: functions: crc: {
	category: "Checksum"
	description: """
		Calculates a CRC of the `value`.
		The CRC `algorithm` used can be optionally specified.

		This function is infallible if either the default `algorithm` value or a recognized-valid compile-time
		`algorithm` string literal is used. Otherwise, it is fallible.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to calculate the checksum for."
			required:    true
			type: ["string"]
		},
		{
			name:        "algorithm"
			description: "The CRC algorithm to use."
			enum: {
				"CRC_3_GSM":                "3-bit CRC used in GSM telecommunications for error detection"
				"CRC_3_ROHC":               "3-bit CRC used in Robust Header Compression (ROHC) protocol"
				"CRC_4_G_704":              "4-bit CRC specified in ITU-T G.704 for synchronous communication systems"
				"CRC_4_INTERLAKEN":         "4-bit CRC used in Interlaken high-speed serial communication protocol"
				"CRC_5_EPC_C1G2":           "5-bit CRC used in EPC Gen 2 RFID (Radio-Frequency Identification) standard"
				"CRC_5_G_704":              "5-bit CRC variant in ITU-T G.704 telecommunication standard"
				"CRC_5_USB":                "5-bit CRC used in USB communication for detecting transmission errors"
				"CRC_6_CDMA2000_A":         "6-bit CRC variant used in CDMA2000 network protocols"
				"CRC_6_CDMA2000_B":         "Alternative 6-bit CRC variant for CDMA2000 network protocols"
				"CRC_6_DARC":               "6-bit CRC used in DARC (Digital Audio Radio Channel) communication"
				"CRC_6_GSM":                "6-bit CRC variant used in GSM telecommunications"
				"CRC_6_G_704":              "6-bit CRC specified in ITU-T G.704 for synchronous communication"
				"CRC_7_MMC":                "7-bit CRC used in MultiMediaCard (MMC) storage systems for error detection"
				"CRC_7_ROHC":               "7-bit CRC used in Robust Header Compression (ROHC) protocol"
				"CRC_7_UMTS":               "7-bit CRC used in UMTS (Universal Mobile Telecommunications System)"
				"CRC_8_AUTOSAR":            "8-bit CRC used in AUTOSAR (Automotive Open System Architecture) standard"
				"CRC_8_BLUETOOTH":          "8-bit CRC polynomial used in Bluetooth communication protocols"
				"CRC_8_CDMA2000":           "8-bit CRC used in CDMA2000 cellular communication standard"
				"CRC_8_DARC":               "8-bit CRC used in DARC (Digital Audio Radio Channel) communication"
				"CRC_8_DVB_S2":             "8-bit CRC used in DVB-S2 (Digital Video Broadcasting Satellite Second Generation)"
				"CRC_8_GSM_A":              "8-bit CRC variant A used in GSM telecommunications"
				"CRC_8_GSM_B":              "8-bit CRC variant B used in GSM telecommunications"
				"CRC_8_HITAG":              "8-bit CRC used in Hitag RFID and transponder systems"
				"CRC_8_I_432_1":            "8-bit CRC specified in IEEE 1432.1 standard"
				"CRC_8_I_CODE":             "8-bit CRC used in I-CODE RFID systems"
				"CRC_8_LTE":                "8-bit CRC used in LTE (Long-Term Evolution) cellular networks"
				"CRC_8_MAXIM_DOW":          "8-bit CRC used by Maxim/Dallas Semiconductor for 1-Wire and iButton devices"
				"CRC_8_MIFARE_MAD":         "8-bit CRC used in MIFARE MAD (Multiple Application Directory) protocol"
				"CRC_8_NRSC_5":             "8-bit CRC used in NRSC-5 digital radio broadcasting standard"
				"CRC_8_OPENSAFETY":         "8-bit CRC used in OpenSAFETY industrial communication protocol"
				"CRC_8_ROHC":               "8-bit CRC used in Robust Header Compression (ROHC) protocol"
				"CRC_8_SAE_J1850":          "8-bit CRC used in SAE J1850 automotive communication protocol"
				"CRC_8_SMBUS":              "8-bit CRC used in System Management Bus (SMBus) communication"
				"CRC_8_TECH_3250":          "8-bit CRC used in SMPTE (Society of Motion Picture and Television Engineers) standard"
				"CRC_8_WCDMA":              "8-bit CRC used in WCDMA (Wideband Code Division Multiple Access) networks"
				"CRC_10_ATM":               "10-bit CRC used in ATM (Asynchronous Transfer Mode) cell headers"
				"CRC_10_CDMA2000":          "10-bit CRC used in CDMA2000 cellular communication standard"
				"CRC_10_GSM":               "10-bit CRC variant used in GSM telecommunications"
				"CRC_11_FLEXRAY":           "11-bit CRC used in FlexRay automotive communication protocol"
				"CRC_11_UMTS":              "11-bit CRC used in UMTS (Universal Mobile Telecommunications System)"
				"CRC_12_CDMA2000":          "12-bit CRC used in CDMA2000 cellular communication standard"
				"CRC_12_DECT":              "12-bit CRC used in DECT (Digital Enhanced Cordless Telecommunications) standards"
				"CRC_12_GSM":               "12-bit CRC variant used in GSM telecommunications"
				"CRC_12_UMTS":              "12-bit CRC used in UMTS (Universal Mobile Telecommunications System)"
				"CRC_13_BBC":               "13-bit CRC used in BBC (British Broadcasting Corporation) digital transmission"
				"CRC_14_DARC":              "14-bit CRC used in DARC (Digital Audio Radio Channel) communication"
				"CRC_14_GSM":               "14-bit CRC variant used in GSM telecommunications"
				"CRC_15_CAN":               "15-bit CRC used in CAN (Controller Area Network) automotive communication"
				"CRC_15_MPT1327":           "15-bit CRC used in MPT 1327 radio trunking system"
				"CRC_16_ARC":               "16-bit CRC used in ARC (Adaptive Routing Code) communication"
				"CRC_16_CDMA2000":          "16-bit CRC used in CDMA2000 cellular communication standard"
				"CRC_16_CMS":               "16-bit CRC used in Content Management Systems for data integrity"
				"CRC_16_DDS_110":           "16-bit CRC used in DDS (Digital Data Storage) standard"
				"CRC_16_DECT_R":            "16-bit CRC variant R used in DECT communication"
				"CRC_16_DECT_X":            "16-bit CRC variant X used in DECT communication"
				"CRC_16_DNP":               "16-bit CRC used in DNP3 (Distributed Network Protocol) for utilities"
				"CRC_16_EN_13757":          "16-bit CRC specified in EN 13757 for meter communication"
				"CRC_16_GENIBUS":           "16-bit CRC used in GENIBUS communication protocol"
				"CRC_16_GSM":               "16-bit CRC variant used in GSM telecommunications"
				"CRC_16_IBM_3740":          "16-bit CRC used in IBM 3740 data integrity checks"
				"CRC_16_IBM_SDLC":          "16-bit CRC used in IBM SDLC (Synchronous Data Link Control)"
				"CRC_16_ISO_IEC_14443_3_A": "16-bit CRC used in ISO/IEC 14443-3 Type A contactless smart cards"
				"CRC_16_KERMIT":            "16-bit CRC used in Kermit file transfer protocol"
				"CRC_16_LJ1200":            "16-bit CRC used in LJ1200 communication system"
				"CRC_16_M17":               "16-bit CRC used in M17 digital radio communication"
				"CRC_16_MAXIM_DOW":         "16-bit CRC used by Maxim/Dallas Semiconductor for data integrity"
				"CRC_16_MCRF4XX":           "16-bit CRC used in MCRF4XX RFID systems"
				"CRC_16_MODBUS":            "16-bit CRC used in Modbus communication protocol for error detection"
				"CRC_16_NRSC_5":            "16-bit CRC used in NRSC-5 digital radio broadcasting standard"
				"CRC_16_OPENSAFETY_A":      "16-bit CRC variant A in OpenSAFETY industrial communication"
				"CRC_16_OPENSAFETY_B":      "16-bit CRC variant B in OpenSAFETY industrial communication"
				"CRC_16_PROFIBUS":          "16-bit CRC used in PROFIBUS industrial communication protocol"
				"CRC_16_RIELLO":            "16-bit CRC used in Riello UPS communication"
				"CRC_16_SPI_FUJITSU":       "16-bit CRC used in Fujitsu SPI (Serial Peripheral Interface) communication"
				"CRC_16_T10_DIF":           "16-bit CRC used in T10 DIF (Data Integrity Field) standard"
				"CRC_16_TELEDISK":          "16-bit CRC used in Teledisk disk image format"
				"CRC_16_TMS37157":          "16-bit CRC used in TMS37157 microcontroller communication"
				"CRC_16_UMTS":              "16-bit CRC used in UMTS (Universal Mobile Telecommunications System)"
				"CRC_16_USB":               "16-bit CRC used in USB communication for error detection"
				"CRC_16_XMODEM":            "16-bit CRC used in XMODEM file transfer protocol"
				"CRC_17_CAN_FD":            "17-bit CRC used in CAN FD (Flexible Data-Rate) automotive communication protocol"
				"CRC_21_CAN_FD":            "21-bit CRC variant used in CAN FD (Flexible Data-Rate) automotive communication"
				"CRC_24_BLE":               "24-bit CRC used in Bluetooth Low Energy (BLE) packet error checking"
				"CRC_24_FLEXRAY_A":         "24-bit CRC variant A used in FlexRay automotive communication protocol"
				"CRC_24_FLEXRAY_B":         "24-bit CRC variant B used in FlexRay automotive communication protocol"
				"CRC_24_INTERLAKEN":        "24-bit CRC used in Interlaken high-speed serial communication protocol"
				"CRC_24_LTE_A":             "24-bit CRC variant A used in LTE (Long-Term Evolution) cellular networks"
				"CRC_24_LTE_B":             "24-bit CRC variant B used in LTE (Long-Term Evolution) cellular networks"
				"CRC_24_OPENPGP":           "24-bit CRC used in OpenPGP (Pretty Good Privacy) for data integrity"
				"CRC_24_OS_9":              "24-bit CRC used in OS-9 operating system for error detection"
				"CRC_30_CDMA":              "30-bit CRC used in CDMA (Code Division Multiple Access) communication standard"
				"CRC_31_PHILIPS":           "31-bit CRC used in Philips communication protocols"
				"CRC_32_AIXM":              "32-bit CRC used in Aeronautical Information Exchange Model (AIXM)"
				"CRC_32_AUTOSAR":           "32-bit CRC used in AUTOSAR (Automotive Open System Architecture) standard"
				"CRC_32_BASE91_D":          "32-bit CRC variant used in Base91 data encoding"
				"CRC_32_BZIP2":             "32-bit CRC used in bzip2 compression algorithm"
				"CRC_32_CD_ROM_EDC":        "32-bit CRC used for Error Detection Code in CD-ROM systems"
				"CRC_32_CKSUM":             "32-bit CRC used in UNIX cksum command for file integrity"
				"CRC_32_ISCSI":             "32-bit CRC used in iSCSI (Internet Small Computer Systems Interface)"
				"CRC_32_ISO_HDLC":          "32-bit CRC used in ISO HDLC (High-Level Data Link Control)"
				"CRC_32_JAMCRC":            "32-bit CRC variant used in JAM error detection"
				"CRC_32_MEF":               "32-bit CRC used in Metro Ethernet Forum (MEF) standards"
				"CRC_32_MPEG_2":            "32-bit CRC used in MPEG-2 transport streams for error detection"
				"CRC_32_XFER":              "32-bit CRC used in data transfer protocols"
				"CRC_40_GSM":               "40-bit CRC variant used in GSM telecommunications"
				"CRC_64_ECMA_182":          "64-bit CRC specified in ECMA-182 standard"
				"CRC_64_GO_ISO":            "64-bit CRC used in Go programming language and ISO standards"
				"CRC_64_MS":                "64-bit CRC variant used in Microsoft systems"
				"CRC_64_REDIS":             "64-bit CRC used in Redis key-value data store"
				"CRC_64_WE":                "64-bit CRC variant for wide-area error detection"
				"CRC_64_XZ":                "64-bit CRC used in the XZ compression format for integrity verification"
				"CRC_82_DARC":              "82-bit CRC used in DARC (Digital Audio Radio Channel) communication"
			}
			required: false
			default:  "CRC_32_ISO_HDLC"
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a string.",
		"`algorithm` is not a supported algorithm.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Create CRC checksum using the default algorithm"
			source: #"""
				crc("foo")
				"""#
			return: "2356372769"
		},
		{
			title: "Create CRC checksum using the CRC_32_CKSUM algorithm"
			source: #"""
				crc("foo", algorithm: "CRC_32_CKSUM")
				"""#
			return: "4271552933"
		},
	]
}
