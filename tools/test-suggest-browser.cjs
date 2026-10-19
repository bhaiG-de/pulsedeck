// Exercise the real Suggest surface with deterministic native responses; no audio device.
const {chromium}=require(process.env.PULSEDECK_PLAYWRIGHT || 'playwright');
(async()=>{
 const browser=await chromium.launch({headless:true,...(process.env.PULSEDECK_BROWSER?{executablePath:process.env.PULSEDECK_BROWSER}:{})});
 const page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.stack||String(e)));
 await page.addInitScript(()=>{
  let sequence=0;const callbacks=new Map(),listeners=new Map();window.__requests=[];
  window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(event,id)=>listeners.get(event)?.delete(id)};
  const engine={running:true,load_revisions:[1,0],deck_control_revisions:[0,0],decks:[{playing:true,position_seconds:60,actual_playback_rate:1,channel_fader:1,bpm:123,tempo_percent:0,position_revision:0,grid_revision:0,beatgrid_valid:true,hot_cues:Array(8).fill(null),memory_cues:Array(10).fill(null)},{playing:false,position_seconds:0,actual_playback_rate:1,channel_fader:0,tempo_percent:0,position_revision:0,grid_revision:0,hot_cues:Array(8).fill(null),memory_cues:Array(10).fill(null)}],sample_rate:48000,metadata_revision:0};
  window.__fixtureEngine=engine;
  const track={id:1,path:'/fixture/current.wav',title:'Current melodic track',artist:'Fixture artist',bpm:123,duration_seconds:300};
  const options=['eq_blend','filter_blend','short_exchange','echo_out','reverb_exit'].map((technique,i)=>({id:`option-${i}`,source:`/fixture/next-${i}.wav`,title:['Aurora','Inner Space','Arrival','Echo arrival','Reverb arrival'][i],artist:'Fixture producer',spotify:false,reference_deck:0,target_deck:1,technique,overlap_bars:[16,8,4,8,8][i],entry_seconds:0,launch_seconds:180,audible_seconds:181,handover_seconds:196,clear_seconds:212,target_bpm:123,native_bpm:124,tempo_percent:-0.806,review_required:true,evidence:['Eight-bar grid prior; phrase unconfirmed','Local pulse spread 0.5% / 0.6%','123 → 124 native BPM; 123 playing BPM'],warnings:['Vocal activity and foreground phrases are unknown; listen before arming'],launch_in_seconds:120,prepare_within_seconds:95,preparation_budget_seconds:25,readiness:'suggested',outgoing:{},incoming:{},score_components:{}}));
  let queue=null;
  let indexed=false;let state='ready';let operationId='prepared-1';
  window.__emitMix=(payload)=>{state=payload.state;for(const callback of listeners.get('mix-updated')?.values()||[])callback({event:'mix-updated',payload:{operation_id:operationId,...payload}});};
  window.__TAURI_INTERNALS__={transformCallback:fn=>{callbacks.set(++sequence,fn);return sequence},unregisterCallback:id=>callbacks.delete(id),invoke:async(command,args)=>{
   if(command==='plugin:event|listen'){if(!listeners.has(args.event))listeners.set(args.event,new Map());listeners.get(args.event).set(args.handler,callbacks.get(args.handler));return args.handler;}
   if(command.startsWith('plugin:event')||command==='ui_publish'||command==='ui_reply')return null;
   if(command!=='control_request')return null;const {command:method,args:a={}}=args.request;window.__requests.push({method,args:a});let result=null;
   if(method==='app.state')result={engine,library:[],playlists:[],audio_devices:[],midi:{},analysis:{active:false,total:0,completed:0,errors:[]},smart_modes:{},spotify_decks:[]};
   else if(method==='app.activity')result={audio_devices:[],midi:{},analysis:{active:false,total:0,completed:0,errors:[]},smart_modes:{},spotify_decks:[]};
   else if(method==='engine.transport')result=engine;
   else if(method==='engine.metadata')result={revision:0,load_revisions:[1,0],tracks:[track,null],samples:[]};
   else if(method==='library.tracks')result={tracks:[],total:0,next_offset:null};
   else if(method==='playlist.list'||method==='library.tag_categories'||method==='spotify.devices'||method==='spotify.analysis.get')result=[];
   else if(method==='spotify.status'||method==='spotify.native.status')result={configured:true,connected:true,playback_permission:true};
   else if(method==='spotify.liked'||method==='spotify.playlist_tracks')result={items:[],total:0,offset:0,next_offset:null};
   else if(method==='spotify.playlists')result={items:[],total:0,offset:0,next_offset:null};
   else if(method==='spotify.analysis.status')result={active:false,total:0,completed:0,cached:0,failed:0};
   else if(method==='recording.status')result={active:false};
   else if(method==='controller.surface')result={tempo_ranges:[10,10],pad_modes:['hotcue','hotcue']};
   else if(method==='mix.options')result={version:1,reference_deck:0,target_deck:1,reference_title:track.title,reference_position_seconds:60,options,coverage:{total_members:1489,indexed_members:indexed?1489:50,analysed_members:24,needs_analysis_members:indexed?1465:26,complete:indexed,needs_analysis:[{source:'spotify:track:unknown',title:'Unanalysed older track',artist:'Older artist'}],needs_analysis_truncated:false},capabilities:[{technique:'eq_blend',available:true,reason:'Guarded native EQ and fader stages'},{technique:'filter_blend',available:true,reason:'Filter and fader stages'},{technique:'short_exchange',available:true,reason:'Four-bar overlap'},{technique:'echo_out',available:true,reason:'Independent echo return'},{technique:'reverb_exit',available:true,reason:'Independent reverb return'}],reasons:[],elapsed_ms:2};
   else if(method==='mix.index_scope'){indexed=true;result={complete:true};}
   else if(method==='mix.queue.status')result=queue||{queue_id:null,state:'idle',revision:0,terminal:true,items:[]};
   else if(method==='mix.queue.start'){queue={queue_id:'queue-1',revision:1,state:'running',terminal:false,mode:'ordered',completed_moves:0,started_unix_ms:1,items:a.sources.map((source,i)=>({id:i+1,source,title:options.find(o=>o.source===source).title,state:i===0?'armed':'pending',availability:'ready'}))};result=queue;}
   else if(method==='mix.queue.edit'){if(a.expected_revision!==queue.revision)throw Error('Edit used stale queue revision');queue={...queue,revision:queue.revision+1,items:[...queue.items.filter(i=>i.state!=='pending'),...a.sources.map(source=>queue.items.find(i=>i.source===source))]};result=queue;}
   else if(method==='mix.queue.stop'){queue={...queue,revision:queue.revision+1,state:'stopped',terminal:true};result=queue;}
   else if(method==='mix.execute'){operationId='execute-1';state='preparing';result={prepared_id:operationId,operation_id:operationId,state,automatic:true,terminal:false,option:options[0]};}
   else if(method==='mix.prepare')result={prepared_id:'prepared-1',operation_id:'prepared-1',state:'ready',ready:true,option:options[0]};
   else if(method==='mix.preview'){state=a.enabled?'previewing':'ready';result={prepared_id:'prepared-1',operation_id:'prepared-1',state,option:options[0]};}
   else if(method==='mix.arm'){state='armed';result={prepared_id:'prepared-1',operation_id:'prepared-1',state};}
   else if(method==='mix.status')result={prepared_id:operationId,operation_id:operationId,state,option:options[0],automatic:operationId==='execute-1',terminal:['completed','cancelled'].includes(state)};
   else if(method==='mix.cancel'){state='cancelled';result={operation_id:'prepared-1',state};}
   return {ok:true,result};
  }};
 });
 await page.goto(process.env.PULSEDECK_UI_URL||'http://127.0.0.1:1430');
 await page.getByRole('button',{name:'Suggest',exact:true}).click();
 await page.getByRole('heading',{name:'Next moves',exact:true}).waitFor();
 await page.getByRole('heading',{name:'Aurora',exact:true}).waitFor();
 if(await page.locator('.suggest-option').count()!==5)throw Error('Expected five distinct technique options');
 const prepare=page.getByRole('button',{name:'Prepare',exact:true});if(!await prepare.isEnabled())throw Error('Preparation requires review before source is loaded');if(await page.getByRole('button',{name:'Arm transition',exact:true}).isEnabled())throw Error('Unprepared option enabled arming');await prepare.click();await page.getByRole('button',{name:'Headphone preview',exact:true}).click();await page.getByRole('button',{name:'Stop preview',exact:true}).click();
 await page.locator('.suggest-option').nth(1).click();if(await page.getByRole('button',{name:'Arm transition',exact:true}).isEnabled())throw Error('Different choice could arm old preparation');await page.locator('.suggest-option').nth(0).click();await page.getByRole('checkbox').check();await page.getByRole('button',{name:'Arm transition',exact:true}).click();
 await page.getByText('Transition: armed',{exact:true}).waitFor();
 await page.getByRole('button',{name:'Cancel',exact:true}).click();
 await page.getByText('Transition: cancelled',{exact:true}).waitFor();
 await page.getByRole('button',{name:'Execute move',exact:true}).click();
 await page.getByText('Transition: preparing',{exact:true}).waitFor();
 await page.evaluate(()=>window.__emitMix({state:'armed'}));await page.getByText('Transition: armed',{exact:true}).waitFor();
 await page.evaluate(()=>window.__emitMix({state:'transitioning'}));await page.getByText('Transition: transitioning',{exact:true}).waitFor();
 const statusCalls=await page.evaluate(()=>window.__requests.filter(r=>r.method==='mix.status').length);
 await page.waitForTimeout(1600);
 if(await page.evaluate(()=>window.__requests.filter(r=>r.method==='mix.status').length)!==statusCalls)throw Error('Move feedback polled status');
 await page.evaluate(()=>window.__emitMix({state:'completed',terminal:true}));await page.getByText('Transition: completed',{exact:true}).waitFor();
 if(await page.evaluate(()=>window.__requests.filter(r=>r.method==='mix.execute').length)!==1)throw Error('Expected one execute call');
 await page.locator('.suggest-filters select').nth(0).selectOption('liked');
 await page.getByRole('button',{name:'Index all Liked Songs',exact:true}).click();
 await page.getByText(/1489 \/ 1489 indexed/).waitFor();
 await page.locator('.suggest-filters select').nth(2).selectOption('filter_blend');
 if(await page.locator('.suggest-option').count()!==1)throw Error('Technique filtering failed');
 await page.locator('.suggest-filters select').nth(2).selectOption('all');
 const queuePanel=page.locator('.suggest-successor-queue');
 await queuePanel.locator('summary').click();
 await queuePanel.getByRole('button',{name:'Add selected track to queue',exact:true}).click();
 await page.locator('.suggest-option').nth(1).click();
 await queuePanel.getByRole('button',{name:'Add selected track to queue',exact:true}).click();
 await queuePanel.getByRole('button',{name:'Move Inner Space earlier',exact:true}).click();
 await queuePanel.getByRole('checkbox').nth(0).check();
 await queuePanel.getByRole('button',{name:'Start successor queue',exact:true}).click();
 await queuePanel.getByRole('status').filter({hasText:'Queue: running'}).waitFor();
 if(await page.getByRole('button',{name:'Execute move',exact:true}).isEnabled())throw Error('Competing move enabled during queue');
 await queuePanel.getByRole('button',{name:'Remove Aurora from queue',exact:true}).click();
 if(await queuePanel.locator('li').count()!==1)throw Error('Queue edit did not remove pending source');
 const queueReads=await page.evaluate(()=>window.__requests.filter(r=>r.method==='mix.queue.status').length);
 await page.waitForTimeout(800);
 if(await page.evaluate(()=>window.__requests.filter(r=>r.method==='mix.queue.status').length)!==queueReads)throw Error('Queue feedback polled status');
 await queuePanel.getByRole('button',{name:'Stop successor queue',exact:true}).click();
 await queuePanel.getByRole('status').filter({hasText:'Queue: stopped'}).waitFor();
 if(await page.evaluate(()=>window.__requests.filter(r=>r.method==='mix.queue.start').length)!==1)throw Error('Queue started more than once');
 await page.evaluate(()=>window.__fixtureEngine.decks[0].position_seconds=70);await page.waitForTimeout(500);if(await page.locator('.suggest-countdown>strong').innerText()!=='110s')throw Error('Countdown did not follow current transport');await page.locator('.suggest-detail').evaluate(e=>e.scrollTop=0);await page.screenshot({path:process.env.PULSEDECK_SUGGEST_SCREENSHOT||'/tmp/pulsedeck-suggest-ui.png',fullPage:true});
 const requests=await page.evaluate(()=>window.__requests);for(const method of ['mix.options','mix.execute','mix.prepare','mix.arm','mix.cancel','mix.index_scope'])if(!requests.some(r=>r.method===method))throw Error('Missing native call '+method);
 if(requests.some(r=>r.method==='engine.command'))throw Error('UI bypassed planner execution API');
 if(errors.length)throw Error(errors.join('\n'));
 await page.setViewportSize({width:800,height:1000});await page.screenshot({path:'/tmp/pulsedeck-suggest-narrow.png',fullPage:true});
 console.log(JSON.stringify({passed:true,options:5,techniques:5,successor_queue_start_edit_stop:true,no_queue_polling:true,prepare_arm_cancel:true,one_call_execution:true,event_feedback:true,no_move_polling:true,indexing:true,errors}));await browser.close();
})().catch(e=>{console.error(e);process.exit(1)});
