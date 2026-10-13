// Read-only CoreAudio/CoreMIDI enumeration. No playback, MIDI send or settings writes.
import Foundation
import CoreAudio
import CoreMIDI

var failures: [[String: Any]] = []

func address(_ selector: AudioObjectPropertySelector, _ scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress(mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain)
}

func bytes(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector, _ scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal) -> Data? {
    var prop = address(selector, scope)
    guard AudioObjectHasProperty(object, &prop) else { return nil }
    var size: UInt32 = 0
    var status = AudioObjectGetPropertyDataSize(object, &prop, 0, nil, &size)
    guard status == noErr else {
        failures.append(["object": object, "selector": selector, "scope": scope, "status": status])
        return nil
    }
    if size == 0 { return Data() }
    var data = Data(count: Int(size))
    status = data.withUnsafeMutableBytes { raw in
        AudioObjectGetPropertyData(object, &prop, 0, nil, &size, raw.baseAddress!)
    }
    guard status == noErr else {
        failures.append(["object": object, "selector": selector, "scope": scope, "status": status])
        return nil
    }
    data.count = Int(size)
    return data
}

func scalar<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector, _ scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal, as type: T.Type) -> T? {
    guard let data = bytes(object, selector, scope), data.count >= MemoryLayout<T>.size else { return nil }
    return data.withUnsafeBytes { $0.loadUnaligned(as: type) }
}

func list<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector, _ scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal, as type: T.Type) -> [T] {
    guard let data = bytes(object, selector, scope) else { return [] }
    let stride = MemoryLayout<T>.stride
    return data.withUnsafeBytes { raw in
        (0..<(data.count / stride)).map { raw.loadUnaligned(fromByteOffset: $0 * stride, as: type) }
    }
}

func stringProperty(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> String? {
    var prop = address(selector)
    guard AudioObjectHasProperty(object, &prop) else { return nil }
    var value: Unmanaged<CFString>? = nil
    var size = UInt32(MemoryLayout.size(ofValue: value))
    let status = AudioObjectGetPropertyData(object, &prop, 0, nil, &size, &value)
    guard status == noErr, let value else {
        failures.append(["object": object, "selector": selector, "status": status])
        return nil
    }
    return value.takeRetainedValue() as String
}

func format(_ value: AudioStreamBasicDescription) -> [String: Any] {
    let id = value.mFormatID
    let chars = [UInt8((id >> 24) & 255), UInt8((id >> 16) & 255), UInt8((id >> 8) & 255), UInt8(id & 255)]
    return ["sample_rate": value.mSampleRate, "format_id": String(bytes: chars, encoding: .ascii) ?? String(id),
            "format_flags": value.mFormatFlags, "channels": value.mChannelsPerFrame,
            "bits_per_channel": value.mBitsPerChannel, "bytes_per_frame": value.mBytesPerFrame,
            "frames_per_packet": value.mFramesPerPacket]
}

func optional<T>(_ value: T?) -> Any { value.map { $0 as Any } ?? NSNull() }

func streams(_ object: AudioObjectID, _ scope: AudioObjectPropertyScope) -> [[String: Any]] {
    list(object, kAudioDevicePropertyStreams, scope, as: AudioStreamID.self).map { stream in
        var row: [String: Any] = ["id": stream]
        row["starting_channel"] = optional(scalar(stream, kAudioStreamPropertyStartingChannel, as: UInt32.self))
        row["latency_frames"] = optional(scalar(stream, kAudioStreamPropertyLatency, as: UInt32.self))
        if let physical = scalar(stream, kAudioStreamPropertyPhysicalFormat, as: AudioStreamBasicDescription.self) { row["physical_format"] = format(physical) }
        if let virtual = scalar(stream, kAudioStreamPropertyVirtualFormat, as: AudioStreamBasicDescription.self) { row["virtual_format"] = format(virtual) }
        row["available_physical_formats"] = list(stream, kAudioStreamPropertyAvailablePhysicalFormats, as: AudioStreamRangedDescription.self).map { range -> [String: Any] in
            ["format": format(range.mFormat), "min_sample_rate": range.mSampleRateRange.mMinimum, "max_sample_rate": range.mSampleRateRange.mMaximum]
        }
        return row
    }
}

let deviceIDs = list(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDevices, as: AudioDeviceID.self)
let devices: [[String: Any]] = deviceIDs.map { device in
    var row: [String: Any] = ["id": device, "name": optional(stringProperty(device, kAudioObjectPropertyName)),
                            "manufacturer": optional(stringProperty(device, kAudioObjectPropertyManufacturer))]
    row["nominal_sample_rate"] = optional(scalar(device, kAudioDevicePropertyNominalSampleRate, as: Double.self))
    row["buffer_frames"] = optional(scalar(device, kAudioDevicePropertyBufferFrameSize, as: UInt32.self))
    if let range = scalar(device, kAudioDevicePropertyBufferFrameSizeRange, as: AudioValueRange.self) { row["buffer_frame_range"] = ["min": range.mMinimum, "max": range.mMaximum] }
    row["available_sample_rates"] = list(device, kAudioDevicePropertyAvailableNominalSampleRates, as: AudioValueRange.self).map { ["min": $0.mMinimum, "max": $0.mMaximum] }
    for (label, scope) in [("input", kAudioObjectPropertyScopeInput), ("output", kAudioObjectPropertyScopeOutput)] {
        row[label] = ["streams": streams(device, scope),
                      "device_latency_frames": optional(scalar(device, kAudioDevicePropertyLatency, scope, as: UInt32.self)),
                      "safety_offset_frames": optional(scalar(device, kAudioDevicePropertySafetyOffset, scope, as: UInt32.self))]
    }
    return row
}

func midiName(_ object: MIDIObjectRef, _ property: CFString) -> Any {
    var value: Unmanaged<CFString>? = nil
    let status = MIDIObjectGetStringProperty(object, property, &value)
    guard status == noErr, let value else { return NSNull() }
    return value.takeRetainedValue() as String
}

func endpoint(_ object: MIDIEndpointRef) -> [String: Any] {
    var id: Int32 = 0
    let status = MIDIObjectGetIntegerProperty(object, kMIDIPropertyUniqueID, &id)
    return ["endpoint_ref": object, "unique_id": status == noErr ? id as Any : NSNull(),
            "name": midiName(object, kMIDIPropertyName), "display_name": midiName(object, kMIDIPropertyDisplayName),
            "manufacturer": midiName(object, kMIDIPropertyManufacturer), "model": midiName(object, kMIDIPropertyModel)]
}

let report: [String: Any] = [
    "captured_at_utc": ISO8601DateFormatter().string(from: Date()),
    "method": "Read-only CoreAudio and CoreMIDI property queries; no streams started or MIDI sent.",
    "audio_devices": devices,
    "midi_sources": (0..<MIDIGetNumberOfSources()).map { endpoint(MIDIGetSource($0)) },
    "midi_destinations": (0..<MIDIGetNumberOfDestinations()).map { endpoint(MIDIGetDestination($0)) },
    "property_errors": failures,
    "limitations": ["Channel indices are API declarations; physical master/headphone assignment needs an output test.",
                    "Current buffer and latency properties are not measured end-to-end response times."]
]
let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
FileHandle.standardOutput.write(data)
FileHandle.standardOutput.write(Data("\n".utf8))
