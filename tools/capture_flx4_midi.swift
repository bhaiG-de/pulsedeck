// Read-only FLX4 MIDI capture. No MIDI output, audio stream or device setting writes.
import Foundation
import CoreMIDI
let seconds = Double(CommandLine.arguments.dropFirst().first ?? "120") ?? 120
func string(_ object: MIDIObjectRef, _ property: CFString) -> String {
 var value: Unmanaged<CFString>?
 guard MIDIObjectGetStringProperty(object, property, &value) == noErr, let value else { return "" }
 return value.takeRetainedValue() as String
}
var client = MIDIClientRef(), port = MIDIPortRef()
let source = (0..<MIDIGetNumberOfSources()).map{MIDIGetSource($0)}.first { string($0,kMIDIPropertyDisplayName).uppercased().contains("DDJ-FLX4") }
guard let source else { fputs("DDJ-FLX4 MIDI source absent\n",stderr); exit(1) }
func line(_ value: [String:Any]) { let data = try! JSONSerialization.data(withJSONObject:value,options:[.sortedKeys]); FileHandle.standardOutput.write(data);FileHandle.standardOutput.write(Data([10])) }
let start = ProcessInfo.processInfo.systemUptime
MIDIClientCreateWithBlock("PulseDeck FLX4 passive validation" as CFString,&client,nil)
MIDIInputPortCreateWithBlock(client,"FLX4 capture" as CFString,&port) { packets,_ in
 let packetOffset = MemoryLayout<MIDIPacketList>.offset(of: \.packet)!
 var pointer = UnsafeRawPointer(packets).advanced(by: packetOffset).assumingMemoryBound(to: MIDIPacket.self)
 for _ in 0..<packets.pointee.numPackets {
  let dataOffset = MemoryLayout<MIDIPacket>.offset(of: \.data)!
  let bytes = Array(UnsafeBufferPointer(start: UnsafeRawPointer(pointer).advanced(by: dataOffset).assumingMemoryBound(to: UInt8.self), count: Int(pointer.pointee.length)))
  line(["elapsed_ms":Int((ProcessInfo.processInfo.systemUptime-start)*1000),"bytes":bytes,"hex":bytes.map{String(format:"%02X",$0)}.joined(separator:" ")])
  pointer = UnsafePointer(MIDIPacketNext(pointer))
 }
}
MIDIPortConnectSource(port,source,nil)
line(["event":"capture_started","source":string(source,kMIDIPropertyDisplayName),"seconds":seconds])
RunLoop.current.run(until:Date(timeIntervalSinceNow:seconds))
MIDIPortDisconnectSource(port,source);MIDIPortDispose(port);MIDIClientDispose(client)
line(["event":"capture_finished"])
