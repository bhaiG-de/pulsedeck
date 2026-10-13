// Automated input for PulseDeck validation only. Never sends to the physical FLX4.
import Foundation
import CoreMIDI
var client = MIDIClientRef(), source = MIDIEndpointRef()
guard MIDIClientCreateWithBlock("PulseDeck validation" as CFString,&client,nil) == noErr,
 MIDISourceCreate(client,"PulseDeck validation" as CFString,&source) == noErr else {exit(1)}
print("Virtual MIDI source ready: PulseDeck validation");fflush(stdout)
DispatchQueue.global().async {
 while let line = readLine() {
  if line == "quit" {break}
  let bytes = line.split(separator:" ").compactMap { UInt8($0,radix:16) }
  guard !bytes.isEmpty,bytes.count<=256 else {continue}
  let memory = UnsafeMutableRawPointer.allocate(byteCount:1024,alignment:16)
  let list = memory.bindMemory(to:MIDIPacketList.self,capacity:1)
  let packet = MIDIPacketListInit(list)
  let ok = bytes.withUnsafeBufferPointer { buf in MIDIPacketListAdd(list,1024,packet,0,bytes.count,buf.baseAddress!) }
  if ok != nil {print("sent \(line): \(MIDIReceived(source,list))");fflush(stdout)}
  memory.deallocate()
 }
 MIDIEndpointDispose(source);MIDIClientDispose(client);exit(0)
}
RunLoop.current.run()
