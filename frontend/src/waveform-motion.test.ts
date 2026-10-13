import { test } from 'node:test';
import assert from 'node:assert/strict';
import { WaveformMotion, type WaveformSnapshot } from './waveform-motion.ts';
const snapshot = (changes: Partial<WaveformSnapshot> = {}): WaveformSnapshot => ({position:10,duration:100,playing:true,playbackRate:1,time:1000,sourceId:'a',positionRevision:1,...changes});
const near = (a:number,b:number) => assert.ok(Math.abs(a-b)<1e-8,`${a} != ${b}`);
test('playback interpolates every display frame at the actual source playback rate and caps stale extrapolation',()=>{
 const m=new WaveformMotion();m.update(snapshot({playbackRate:1.5}));
 near(m.position(1016),10.024);near(m.position(1032),10.048);near(m.position(1120),10.18);near(m.position(9000),10.18);assert.equal(m.animating(9000),false);
});
test('native snapshots correct small clock errors without a backwards 30Hz step, but seeks and pause are authoritative',()=>{
 const m=new WaveformMotion();m.update(snapshot());m.update(snapshot({position:10.025,time:1033}));near(m.position(1033),10.033);near(m.position(1113),10.105);
 m.update(snapshot({position:40,time:1120,positionRevision:2}));near(m.position(1120),40);
 m.update(snapshot({position:40.02,time:1140,positionRevision:2,playing:false}));near(m.position(9000),40.02);
});
test('holding freezes the displayed center and immediate drag feedback ignores delayed native seek acknowledgements',()=>{
 const m=new WaveformMotion();m.update(snapshot());near(m.beginGrab(1050),10.05);near(m.position(1100),10.05);near(m.moveGrab(2),2);
 m.update(snapshot({position:10.05,time:1100,jogTouch:true,positionRevision:2}));near(m.position(1150),12.05);
 m.update(snapshot({position:11,time:1160,jogTouch:true,positionRevision:3}));near(m.position(1200),12.05);
 m.endGrab(1200);m.update(snapshot({position:12.05,time:1210,jogTouch:true,positionRevision:4}));near(m.position(1240),12.05);
 m.update(snapshot({position:12.06,time:1240,jogTouch:false,positionRevision:4}));near(m.position(1240),12.05);near(m.position(1320),12.14);
});
test('grab deltas are clamped before native commands, and release at EOF never extrapolates outside the file',()=>{
 const m=new WaveformMotion();m.update(snapshot({position:99.99}));m.beginGrab(1000);near(m.moveGrab(10),.01);near(m.moveGrab(4),0);near(m.moveGrab(-200),-100);near(m.position(1050),0);
 m.moveGrab(100);m.endGrab(1100);m.update(snapshot({position:100,time:1110,playing:false,positionRevision:2}));near(m.position(9000),100);
});
test('source changes and failed gesture cancellation discard local previews',()=>{
 const m=new WaveformMotion();m.update(snapshot());m.beginGrab(1000);m.moveGrab(4);m.update(snapshot({sourceId:'b',position:0,time:1040,playing:false}));near(m.position(1040),0);
 m.beginGrab(1040);m.moveGrab(20);m.cancelGrab();near(m.position(1040),0);
});
test('overview seek preview waits for a fresh revision, then resumes native source movement',()=>{
 const m=new WaveformMotion();m.update(snapshot());m.expectSeek(40,1050);m.update(snapshot({position:10.07,time:1070}));near(m.position(1070),40);
 m.update(snapshot({position:40,time:1090,positionRevision:2}));near(m.position(1100),40.01);
 m.expectSeek(80,1150);near(m.position(1150),80);near(m.position(1600),40.12);
});
test('loop interpolation wraps source coordinates while unknown or inactive loops do not',()=>{
 const m=new WaveformMotion();m.update(snapshot({position:10.95,loopStart:10,loopEnd:11}));near(m.position(1100),10.05);
 m.update(snapshot({position:10.95,time:1200,loopStart:10,loopEnd:11,loopActive:false}));near(m.position(1320),11.07);
});
