/**
 * Headless CPU/canvas comparison, not browser FPS or end-to-end audio latency.
 * Install an isolated renderer: npm install --prefix /tmp/pulsedeck-wave-benchmark @napi-rs/canvas
 * Run: node frontend/src/waveform-render.bench.ts /tmp/pulsedeck-wave-benchmark/node_modules/@napi-rs/canvas/index.js
 */
import { performance } from 'node:perf_hooks';
import { pathToFileURL } from 'node:url';
import { rebinEnvelope, smoothEnvelope, BAND_COLORS } from './waveform-data.ts';
import { WaveformRaster, type CanvasFactory } from './waveform-renderer.ts';
const modulePath=process.argv[2];
if(!modulePath)throw new Error('Pass the absolute path to an isolated @napi-rs/canvas/index.js');
const {createCanvas}=await import(pathToFileURL(modulePath).href);
const factory:CanvasFactory=(w,h)=>createCanvas(w,h) as HTMLCanvasElement;
const count=4096,duration=377,dpr=2,bpm=122,span=32*60/bpm;
const samples=Array.from({length:count},(_,i)=>.12+.82*Math.abs(Math.sin(i*.031)*Math.cos(i*.007)));
const bands={low:samples.map((p,i)=>p*(.25+.45*Math.abs(Math.sin(i*.011)))),mid:samples.map((p,i)=>p*(.16+.30*Math.abs(Math.cos(i*.017)))),high:samples.map((p,i)=>p*(.05+.17*Math.abs(Math.sin(i*.051))))};
const graphs=[{width:1400,height:108,overview:false},{width:1400,height:108,overview:false},{width:500,height:28,overview:true},{width:500,height:28,overview:true}].map(g=>{
 const canvas=factory(g.width*dpr,g.height*dpr),ctx=canvas.getContext('2d')!;ctx.setTransform(dpr,0,0,dpr,0,0);const raster=new WaveformRaster(factory);
 raster.configure({samples,bands,duration,height:g.height,dpr,pixelsPerSecond:g.width/(g.overview?duration:span),color:'#ffffff',overview:g.overview});return {...g,ctx,raster};
});
function legacyExact(g:typeof graphs[number],start:number){
 const {ctx,width:w,height:h,overview}=g,visible=overview?duration:span,columns=Math.ceil(w),peak=rebinEnvelope(samples,start/duration,(start+visible)/duration,columns),center=h/2,scale=h*.43;
 ctx.fillStyle='#7790ae';ctx.globalAlpha=overview?.18:.14;ctx.beginPath();ctx.moveTo(0,center);for(let i=0;i<columns;i++)ctx.lineTo(i,center-peak[i]*scale);for(let i=columns-1;i>=0;i--)ctx.lineTo(i+1,center+peak[i]*scale);ctx.closePath();ctx.fill();
 const lo=smoothEnvelope(rebinEnvelope(bands.low,start/duration,(start+visible)/duration,columns,'rms')),mi=smoothEnvelope(rebinEnvelope(bands.mid,start/duration,(start+visible)/duration,columns,'rms')),hi=smoothEnvelope(rebinEnvelope(bands.high,start/duration,(start+visible)/duration,columns,'rms'));
 const low={value:0,color:BAND_COLORS.low},mid={value:0,color:BAND_COLORS.mid},high={value:0,color:BAND_COLORS.high},layers=[low,mid,high];ctx.globalAlpha=overview?.82:.92;
 for(let x=0;x<columns;x++){low.value=lo[x];mid.value=mi[x];high.value=hi[x];layers.sort((a,b)=>b.value-a.value);for(const l of layers){if(l.value>0){ctx.fillStyle=l.color;ctx.fillRect(x,center-l.value*scale,1.15,l.value*scale*2)}}}ctx.globalAlpha=1;
}
function frame(mode:'before'|'cached',position:number){for(let i=0;i<graphs.length;i++){const g=graphs[i],start=g.overview?0:position+i*.25-span/2;g.ctx.clearRect(0,0,g.width,g.height);if(mode==='before')legacyExact(g,start);else g.raster.draw(g.ctx,start,g.width);g.ctx.getImageData(0,0,1,1);}}
const coldStart=performance.now();frame('cached',90);const coldMs=performance.now()-coldStart;
function measure(mode:'before'|'cached'){
 for(let i=0;i<24;i++)frame(mode,90+i/60);const buildsBefore=graphs.map(g=>g.raster.tileBuilds),times:number[]=[];
 for(let i=0;i<180;i++){
  const position=90+i/60;if(mode==='cached'&&i%30===0)for(let j=0;j<2;j++)graphs[j].raster.prewarm(position+j*.25-span/2,graphs[j].width);
  const start=performance.now();frame(mode,position);times.push(performance.now()-start);
 }
 times.sort((a,b)=>a-b);const pct=(p:number)=>Number(times[Math.floor((times.length-1)*p)].toFixed(3));return {median_ms:pct(.5),p95_ms:pct(.95),p99_ms:pct(.99),new_tiles_during_run:graphs.map((g,i)=>g.raster.tileBuilds-buildsBefore[i])};
}
const before=measure('before'),after=measure('cached');
console.log(JSON.stringify({fixture:{source_points:count,duration_seconds:duration,detail_width:1400,detail_height:108,overview_width:500,overview_height:28,dpr,graphs:4,measured_frames:180},before,after,cold_first_four_graph_ms:Number(coldMs.toFixed(3)),median_cpu_reduction_factor:Number((before.median_ms/after.median_ms).toFixed(2)),tile_limit_per_graph:6,max_detail_raster_rgba_bytes_per_graph:6*1024*dpr*108*dpr*4,limitations:'Headless Skia canvas layer only. Excludes React, native IPC, browser compositing, grid/cue overlays and physical controller latency. Prewarm tile construction runs outside measured animation frames; cold start includes first index and raster construction.'},null,2));
