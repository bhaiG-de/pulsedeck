// Browser regression for real UI code with fixture native/Spotify responses.
// Serve frontend/dist, set PULSEDECK_PLAYWRIGHT and (optionally)
// PULSEDECK_BROWSER / PULSEDECK_UI_URL, then run this script.
const { chromium } = require(process.env.PULSEDECK_PLAYWRIGHT || 'playwright');
const fs = require('fs');
(async () => {
 const browser = await chromium.launch({headless:true,...(process.env.PULSEDECK_BROWSER ? {executablePath:process.env.PULSEDECK_BROWSER} : {})});
 const page = await browser.newPage({viewport:{width:1500,height:940},deviceScaleFactor:process.env.PULSEDECK_PLAYBACK_TRACK?2:1});
 const errors=[];page.on('pageerror',e=>errors.push(String(e)));
 const playbackTrack=process.env.PULSEDECK_PLAYBACK_TRACK ? JSON.parse(fs.readFileSync(process.env.PULSEDECK_PLAYBACK_TRACK,'utf8')) : null;
 await page.addInitScript((playbackTrack) => {
  const callbacks=new Map(),listeners=new Map();let sequence=0;
  window.__fixtureRequests=[];window.__fixtureUi={};window.__fixtureReplies={};window.__fixtureContext=[];window.__fixtureSelectionErrors=0;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(event,id)=>listeners.get(event)?.delete(id)};
  window.__fixtureEmit=(event,payload)=>{for(const [id,callback] of listeners.get(event)||[])callback({event,id,payload});};
  const playbackStart=performance.now();
  const engine={decks:[0,1].map(i=>({playing:!!playbackTrack,track_id:playbackTrack?.id,track_path:playbackTrack?.path,duration_seconds:playbackTrack?.duration_seconds||0,bpm:playbackTrack?.bpm||120,tempo_percent:0,playback_rate:1,actual_playback_rate:1,position_seconds:i*12,position_revision:1,beatgrid_valid:!!playbackTrack,is_master:i===0,hot_cues:Array(8).fill(null),memory_cues:Array(10).fill(null)})),load_revisions:playbackTrack?[1,1]:[0,0],running:!!playbackTrack,metadata_revision:0,sample_rate:48000};
  window.__fixtureEngine=engine;
  let fixtureDetails={id:1,revision:0,title:'Track 000001',artist:'Fixture Artist',fields:{title:'Track 000001',artist:'Fixture Artist',Genre:'House',Comment:'Original comment',DJ_ENERGY:'Warm'},rating:0,color:'',my_tags:[],file_metadata:[],has_file_edits:false};
  let categories=[{id:1,name:'Energy',position:0,tags:[{id:1,name:'Warmup',position:0,count:0},{id:2,name:'Peak',position:1,count:0}]}];
  function track(i){return {id:i+1,path:`/fixture/${i+1}.mp3`,title:`Track ${String(i+1).padStart(6,'0')}`,artist:'Fixture Artist',duration_seconds:180,bpm:120,key:'A minor',sample_rate:44100,added_at:'1',tags:i===0?['first','test']:[]};}
  const status={configured:true,connected:true,playback_permission:true};
  window.__TAURI_INTERNALS__={transformCallback:(fn)=>{const id=++sequence;callbacks.set(id,fn);return id},unregisterCallback:(id)=>callbacks.delete(id),invoke:async(command,args)=>{
   if(command==='plugin:event|listen'){if(!listeners.has(args.event))listeners.set(args.event,new Map());listeners.get(args.event).set(args.handler,callbacks.get(args.handler));return args.handler;}
   if(command.startsWith('plugin:event'))return 1;
   if(command==='ui_publish'){window.__fixtureUi[args.target]=args.snapshot;return null;}
   if(command==='ui_reply'){window.__fixtureReplies[args.requestId]=args;return null;}
   if(command!=='control_request')return null;
   const {command:method,args:a={}}=args.request;window.__fixtureRequests.push({method,args:a});let result=null;
   if(method==='app.state')result={engine,library:[],playlists:[],audio_devices:[],midi:{},analysis:{active:false,total:0,completed:0,errors:[]},smart_modes:{fader:false,cfx:false},spotify_decks:[]};
   else if(method==='app.activity')result={midi:{},analysis:{active:false,total:0,completed:0,errors:[]},smart_modes:{fader:false,cfx:false},spotify_decks:[],audio_devices:[]};
   else if(method==='engine.transport'){if(playbackTrack&&!window.__fixtureFrozen)engine.decks.forEach((deck,i)=>deck.position_seconds=(performance.now()-playbackStart)/1000+i*12);result=engine;}
   else if(method==='engine.metadata')result={revision:0,load_revisions:engine.load_revisions,tracks:playbackTrack?[playbackTrack,playbackTrack]:[null,null],samples:[]};
   else if(method==='controller.context'){await new Promise(resolve=>setTimeout(resolve,25));window.__fixtureContext=a.track_ids;if(a.selected_track_id!==undefined){if(!a.track_ids.includes(a.selected_track_id))throw Error('Invalid atomic selection');window.__fixtureSelected=a.selected_track_id;}}
   else if(method==='controller.selection'&&window.__fixtureContext.length&&!window.__fixtureContext.includes(a.track_id)){window.__fixtureSelectionErrors++;return {ok:false,error:{message:'Selected track is not in the current browser'}};}
   else if(method==='library.tag_categories')result=structuredClone(categories);
   else if(method==='library.details')result=structuredClone(fixtureDetails);
   else if(method==='library.edit_details'){window.__fixtureHideSelection=true;fixtureDetails={...fixtureDetails,revision:fixtureDetails.revision+1,has_file_edits:true,fields:{...fixtureDetails.fields,...a.patch.fields},rating:a.patch.rating,color:a.patch.color,my_tags:a.patch.my_tags};result=structuredClone(fixtureDetails);}
   else if(method==='library.write_metadata')result={backup:'/fixture/original.backup',details:structuredClone(fixtureDetails)};
   else if(method==='library.edit_taxonomy'){if(a.kind==='category'&&a.id)categories.find(c=>c.id===a.id).name=a.name;result=structuredClone(categories);}
   else if(method==='library.tracks'){
    let count=100000;let start=a.offset||0;let n=a.limit||200;let out;
    if(a.search){const m=a.search.match(/Track (\d+)/i);out=m?[track(Number(m[1])-1)]:[];count=out.length;out=out.slice(start,start+n);}
    else if(a.filters?.my_tags?.length||a.filters?.rating_min!==undefined||a.filters?.colors?.length){count=1;out=start?[]:[track(0)];}
    else out=Array.from({length:Math.max(0,Math.min(n,count-start))},(_,i)=>track(start+i+(window.__fixtureHideSelection?1:0)));
    result={tracks:out,total:count,duration_seconds:count*180,next_offset:start+n<count?start+n:null};
   }
   else if(method==='playlist.list')result=[];
   else if(method==='spotify.status'||method==='spotify.native.status')result=status;
   else if(method==='spotify.devices')result=[];
   else if(method==='spotify.analysis.status')result={active:false,total:0,completed:0,cached:0,failed:0};
   else if(method==='spotify.analysis.get')result=[];
   else if(method==='spotify.playlists')result={items:[{id:'big',name:'Large Playlist',total:5000,external_url:'',collaborative:false,artwork_url:'https://i.scdn.co/image/fixture'}],total:1,offset:0,next_offset:null};
   else if(method==='spotify.playlist_tracks'||method==='spotify.liked'){
    const start=a.offset||0,total=5000;
    result={items:Array.from({length:Math.min(50,total-start)},(_,i)=>({id:`s${start+i}`,uri:`spotify:track:fixture${start+i}`,title:`Spotify Track ${start+i+1}`,artists:['Fixture Artist'],album:'Fixture Album',duration_ms:180000,available:true,is_local:false,external_url:'',playable_in_deck:false,artwork_url:'https://i.scdn.co/image/fixture'})),total,offset:start,next_offset:start+50<total?start+50:null};
   }
   else if(method==='recording.status')result={active:false};
   else if(method==='controller.surface')result={tempo_ranges:[10,10],pad_modes:['hotcue','hotcue']};
   return {ok:true,result};
  }};
 },playbackTrack);
 await page.route('https://i.scdn.co/**',r=>r.fulfill({contentType:'image/png',body:Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a1ioAAAAASUVORK5CYII=','base64')}));
 await page.goto(process.env.PULSEDECK_UI_URL || 'http://127.0.0.1:1430');
 await page.locator('[data-browser-source="local"]').click();
 await page.locator('[data-track-id]').first().waitFor();
 const pane=page.locator('[data-browser-pane="local-tracks"]');
 const initial=await page.locator('[data-track-id]').count();
 if(initial>60)throw Error('Too many initial rows '+initial);
 await page.waitForTimeout(200);
 for(let i=0;i<8;i++){await pane.evaluate(e=>e.scrollTop=e.scrollHeight);await page.waitForTimeout(150);}
 const rowCount=await page.locator('[data-track-id]').count();
 const req=await page.evaluate(()=>window.__fixtureRequests.filter(r=>r.method==='library.tracks'));
 if(req.length<3)throw Error('No automatic local page loading '+JSON.stringify(req));
 if(rowCount>70)throw Error('DOM grew with pages '+rowCount);
 const beforeMetadata=await pane.evaluate(e=>({scroll:e.scrollTop,requests:window.__fixtureRequests.filter(r=>r.method==='library.tracks').length}));
 await page.evaluate(()=>window.__fixtureEmit('library-updated',{kind:'metadata'}));await page.waitForTimeout(180);
 const afterMetadata=await pane.evaluate(e=>({scroll:e.scrollTop,requests:window.__fixtureRequests.filter(r=>r.method==='library.tracks').length}));
 if(afterMetadata.requests!==beforeMetadata.requests||Math.abs(afterMetadata.scroll-beforeMetadata.scroll)>1)throw Error('Metadata backfill reset unfiltered browsing');
 await page.getByRole('textbox',{name:'Search library'}).fill('Track 099999');
 await page.waitForTimeout(500);
 await page.getByText('Track 099999',{exact:true}).waitFor();
 const beforeSearchRefresh=await page.evaluate(()=>window.__fixtureRequests.filter(r=>r.method==='library.tracks').length);
 await page.evaluate(()=>window.__fixtureEmit('library-updated',{kind:'metadata'}));await page.waitForTimeout(180);
 const afterSearchRefresh=await page.evaluate(()=>window.__fixtureRequests.filter(r=>r.method==='library.tracks').length);
 if(afterSearchRefresh<=beforeSearchRefresh)throw Error('Active metadata search was not refreshed after backfill');
 await page.getByRole('textbox',{name:'Search library'}).fill('');await page.waitForTimeout(500);
 // A paged controller selection must wait for the expanded native context.
 await page.evaluate(async()=>{
  async function invoke(action,value){const request_id='controller-page-'+action;window.__fixtureEmit('product-control',{request_id,target:'app',action,value});for(let n=0;n<300&&!window.__fixtureReplies[request_id];n++)await new Promise(resolve=>setTimeout(resolve,10));const reply=window.__fixtureReplies[request_id];if(!reply||reply.error)throw Error(JSON.stringify(reply));}
  await invoke('selection',200);await invoke('controller-browser',{type:'browse',delta:30,shifted:false});
  if(window.__fixtureContext.length<400||window.__fixtureSelectionErrors||window.__fixtureSelected!==230)throw Error('Controller selected before publishing the next page');
 });
 await pane.evaluate(e=>e.scrollTop=0);
 await page.locator('[data-track-id="1"]').waitFor();
 const input=page.getByRole('textbox',{name:'Add tag to Track 000001'});await input.fill('draft survives');
 if(/1px 2px/.test(await input.evaluate(e=>getComputedStyle(e).boxShadow)))throw Error('Blurred native tag fields returned to the scrolling table');
 await pane.evaluate(e=>e.scrollTop=e.scrollHeight);await page.waitForTimeout(250);
 if(await input.inputValue()!=='draft survives')throw Error('Focused tag draft lost');
 const frames=await pane.evaluate(async e=>{
   const values=[];let previous=performance.now();
   for(let i=0;i<240;i++){await new Promise(requestAnimationFrame);const now=performance.now();if(i>10)values.push(now-previous);previous=now;e.scrollTop=Math.max(0,e.scrollTop-45);}
   values.sort((a,b)=>a-b);return {samples:values.length,p90:values[Math.ceil(values.length*.90)-1],p95:values[Math.ceil(values.length*.95)-1],p99:values[Math.ceil(values.length*.99)-1]};
 });
 const browsingPerformance=await page.evaluate(()=>window.__fixtureUi.performance?.state);
 if(playbackTrack){
  const waves=browsingPerformance.waveforms;
  if(waves['0-detail'].paints>=100)throw Error('Moving waveform is repainting rather than compositing');
 }
 await page.waitForTimeout(250);
 await page.screenshot({path:'/tmp/pulsedeck-local-virtual.png'});

 // The new controls must send compact filter queries and revision-checked patches.
 await pane.evaluate(e=>e.scrollTop=0);await page.waitForTimeout(150);await page.locator('[data-track-id="1"] td').first().click();
 await page.getByRole('button',{name:'Edit metadata',exact:true}).click();const editor=page.getByRole('dialog',{name:'Edit track metadata'});await editor.waitFor();
 await editor.getByRole('textbox',{name:'Genre',exact:true}).fill('Afro House');await editor.getByLabel('Track rating',{exact:true}).selectOption('4');await editor.getByLabel('Track color',{exact:true}).selectOption('#FF0000');
 await editor.getByText('Energy',{exact:true}).click();await editor.getByLabel('Assign Energy: Warmup',{exact:true}).check();await editor.getByRole('button',{name:'Save to library',exact:true}).click();await editor.getByRole('status').filter({hasText:'Saved in library'}).waitFor();
 await editor.getByText('Custom fields and embedded originals',{exact:true}).click();await editor.getByLabel('Custom metadata key',{exact:true}).fill('DJ_FLOOR');await editor.getByLabel('Custom metadata value',{exact:true}).fill('Late night');await editor.getByRole('button',{name:'Save to library',exact:true}).click();await page.waitForTimeout(150);
 if(process.env.PULSEDECK_TAGGING_SCREENSHOT)await page.screenshot({path:process.env.PULSEDECK_TAGGING_SCREENSHOT});
 await editor.getByRole('button',{name:'Write saved text edits to file',exact:true}).click();await editor.getByRole('status').filter({hasText:'Original backup'}).waitFor();await editor.getByRole('button',{name:'Close metadata editor',exact:true}).click();
 await page.getByRole('button',{name:'Filters / My Tags',exact:true}).click();await page.getByLabel('Minimum rating',{exact:true}).selectOption('4');await page.getByLabel('Filter color Red',{exact:true}).check();
 await page.locator('.my-tag-categories details').first().locator('summary').click();await page.getByLabel('Filter Energy: Warmup',{exact:true}).check();await page.getByLabel('My Tag match mode',{exact:true}).selectOption('or');await page.waitForTimeout(250);
 const facetRequest=await page.evaluate(()=>window.__fixtureRequests.filter(r=>r.method==='library.tracks').at(-1));
 if(facetRequest.args.filters.tag_mode!=='or'||facetRequest.args.filters.rating_min!==4||facetRequest.args.filters.colors[0]!=='#FF0000'||facetRequest.args.filters.my_tags[0]!==1)throw Error('Facet request incorrect '+JSON.stringify(facetRequest));
 const editRequests=await page.evaluate(()=>window.__fixtureRequests.filter(r=>r.method==='library.edit_details'));if(editRequests.length!==2||editRequests[0].args.patch.fields.Genre!=='Afro House'||editRequests[0].args.patch.my_tags[0]!==1||editRequests[1].args.patch.fields.DJ_FLOOR!=='Late night')throw Error('Metadata patch incorrect');
 if(playbackTrack){
  await page.locator('[data-track-id="1"] td').first().click();await page.getByRole('button',{name:'Edit metadata',exact:true}).click();await editor.waitFor();
  await page.evaluate(()=>{window.__stressFrames=[];window.__stressRunning=true;let last=performance.now();function sample(now){window.__stressFrames.push(now-last);last=now;if(window.__stressRunning)requestAnimationFrame(sample)}requestAnimationFrame(sample)});
  for(let i=0;i<30;i++){
   await editor.getByRole('textbox',{name:'Genre',exact:true}).fill('Afro House '+i);
   const group=editor.locator('.metadata-my-tags details').first();if(await group.count() && (await group.getAttribute('open'))===null)await group.locator('summary').click();
   const assigned=editor.getByLabel('Assign Energy: Warmup',{exact:true});await assigned.setChecked(i%2===0);
   await editor.getByRole('button',{name:'Save to library',exact:true}).click();
   await page.waitForFunction(n=>window.__fixtureRequests.filter(r=>r.method==='library.edit_details').length===n,i+3);
   await page.getByLabel('My Tag match mode',{exact:true}).selectOption(i%2?'and':'or');
   await page.getByLabel('Minimum rating',{exact:true}).selectOption(i%2?'3':'4');
  }
  const stress=await page.evaluate(()=>{window.__stressRunning=false;const f=window.__stressFrames.sort((a,b)=>a-b);return {saves:window.__fixtureRequests.filter(r=>r.method==='library.edit_details').length-2,frames:f.length,p95_ms:f[Math.floor(f.length*.95)],p99_ms:f[Math.floor(f.length*.99)],max_ms:f.at(-1),bothPlaying:window.__fixtureEngine.decks.every(d=>d.playing),performance:window.__fixtureUi.performance?.state}});
  if(stress.saves!==30||!stress.bothPlaying)throw Error('Concurrent edit/filter playback failed '+JSON.stringify(stress));
  fs.writeFileSync('/tmp/pulsedeck-edit-filter-browser-stress.json',JSON.stringify(stress,null,2));
  const counts=await page.evaluate(()=>({pages:window.__fixtureRequests.filter(r=>r.method==='library.tracks').length,playlists:window.__fixtureRequests.filter(r=>r.method==='playlist.list').length}));
  await page.evaluate(()=>window.__fixtureEmit('library-updated',{kind:'library',command:'library.edit_details'}));await page.waitForTimeout(200);
  const refreshed=await page.evaluate(()=>({pages:window.__fixtureRequests.filter(r=>r.method==='library.tracks').length,playlists:window.__fixtureRequests.filter(r=>r.method==='playlist.list').length}));
  if(refreshed.pages<=counts.pages||refreshed.playlists!==counts.playlists)throw Error('Metadata edits must refresh tracks without refetching unchanged playlists');
  await editor.getByRole('button',{name:'Close metadata editor',exact:true}).click();
 }
 await page.getByRole('button',{name:'Clear filters',exact:true}).click();await page.getByText('Manage categories and tags',{exact:true}).click();const manager=page.locator('.taxonomy-manager>details').first();await manager.locator('summary').click();await page.getByRole('textbox',{name:'Rename category Energy',exact:true}).fill('Vibe');await manager.getByRole('button',{name:'Rename',exact:true}).first().click();await page.getByRole('textbox',{name:'Rename category Vibe',exact:true}).waitFor();
 await page.getByRole('button',{name:'Filters / My Tags',exact:true}).click();await page.waitForTimeout(150);
 await page.getByRole('button',{name:/Large Playlist/}).click();
 await page.getByText('Spotify Track 1',{exact:true}).waitFor();
 const spotifyPane=page.locator('[data-browser-pane="spotify-tracks"]');
 for(let i=0;i<5;i++){await spotifyPane.evaluate(e=>e.scrollTop=e.scrollHeight);await page.waitForTimeout(180);}
 const spotifyRows=await page.locator('.spotify-tracks tr[data-index]').count();
 const spotifyRequests=await page.evaluate(()=>window.__fixtureRequests.filter(r=>r.method==='spotify.playlist_tracks'));
 if(spotifyRequests.length<3)throw Error('No automatic Spotify paging '+JSON.stringify(spotifyRequests));
 if(spotifyRows>70)throw Error('Spotify DOM grew '+spotifyRows);
 if(await page.getByRole('button',{name:/Load more tracks|More playlists/}).count())throw Error('Manual more buttons remain');
 await page.waitForTimeout(250);
 await page.screenshot({path:'/tmp/pulsedeck-spotify-virtual.png'});
 if(playbackTrack){
  const before=await page.evaluate(()=>window.__fixtureUi.performance.state.waveforms['0-detail'].paints);
  await page.evaluate(async()=>{
   for(let i=0;i<60;i++){
    window.__fixtureEngine.decks[0].gain=.7+(i%8)*.05;
    window.__fixtureEngine.control_revision=(window.__fixtureEngine.control_revision||0)+1;
    await new Promise(resolve=>setTimeout(resolve,20));
   }
  });
  await page.waitForTimeout(1100);
  const after=await page.evaluate(()=>window.__fixtureUi.performance.state.waveforms['0-detail'].paints);
  if(after-before>12)throw Error('Mixer changes invalidate unrelated waveform pixels '+JSON.stringify({before,after}));
  const plot=page.locator('.stacked-wave-row .waveform').first();
  const moving=await plot.evaluate(node=>{
   const layers=[...node.querySelectorAll('canvas')];
   return {animations:layers.map(c=>c.getAnimations().filter(a=>a.playState==='running').length),alignment:Math.abs(layers[0].getBoundingClientRect().x-layers[1].getBoundingClientRect().x)};
  });
  if(moving.animations.some(n=>n!==1)||moving.alignment>1)throw Error('Waveform layers lost synchronized compositor motion '+JSON.stringify(moving));
  for(const position of [42, .05, playbackTrack.duration_seconds-.05]){
   await page.evaluate(position=>{
    window.__fixtureFrozen=true;
    for(const d of window.__fixtureEngine.decks){d.playing=false;d.position_seconds=position;d.position_revision++;}
   },position);
   await page.waitForTimeout(180);
   const shown=await plot.evaluate(node=>{
    const c=node.querySelector('canvas'),m=new DOMMatrix(getComputedStyle(c).transform);
    return Number(c.dataset.anchorSeconds)-(m.m41+Number(c.dataset.paddingPixels))/Number(c.dataset.pixelsPerSecond);
   });
   if(Math.abs(shown-position)>.001)throw Error('Seek/edge is visually off the native source clock '+JSON.stringify({shown,position}));
  }
  await page.evaluate(()=>{window.__fixtureFrozen=false;for(const d of window.__fixtureEngine.decks){d.playing=true;d.position_revision++;}});
  await page.waitForTimeout(200);
 }
 if(playbackTrack){
  await page.getByRole('button',{name:'Library',exact:true}).click().catch(()=>{});
  const platter=page.locator('.jog-surface > i').first();
  const motion=await platter.evaluate(async node=>{
   const animation=node.getAnimations()[0];if(!animation)throw Error('No compositor animation for platter');
   const start=Number(animation.currentTime);await new Promise(resolve=>setTimeout(resolve,1000));
   return {state:animation.playState,advanced:Number(animation.currentTime)-start};
  });
  if(motion.state!=='running' || motion.advanced<850 || motion.advanced>1150)throw Error('Platter lost native clock synchronization '+JSON.stringify(motion));
 }
 await page.waitForTimeout(1100);
 const performance=await page.evaluate(()=>window.__fixtureUi.performance?.state);
 const render=performance?.library;
 console.log(JSON.stringify({browsingPerformance,performance,render,frames,localInitialRows:initial,localRowsAfterScrolling:rowCount,localPages:req.length,maxLocalOffset:Math.max(...req.map(r=>r.args.offset||0)),spotifyRows,spotifyPages:spotifyRequests.length,errors},null,2));
 fs.writeFileSync('/tmp/pulsedeck-virtual-ui-results.json',JSON.stringify({browsingPerformance,performance,render,frames,localInitialRows:initial,localRowsAfterScrolling:rowCount,localPages:req.length,spotifyRows,spotifyPages:spotifyRequests.length,errors}));
 await browser.close();if(errors.length)process.exitCode=1;
})().catch(e=>{console.error(e);process.exit(1)});
