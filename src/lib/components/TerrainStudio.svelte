<script lang="ts">
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { AlertTriangle, CheckCircle2, Download, FolderOpen, Grid3X3, Map } from "lucide-svelte";
  import { api, assetUrl } from "$lib/api";
  import { errorMessage, type Asset, type TerrainExportResult, type TerrainRuleMode, type TerrainRuleRole } from "$lib/types";

  const RULE_ROLES: { id: TerrainRuleRole; short: string; label: string }[] = [
    { id: "top_left", short: "NW", label: "Top left" },
    { id: "top", short: "N", label: "Top" },
    { id: "top_right", short: "NE", label: "Top right" },
    { id: "left", short: "W", label: "Left" },
    { id: "center", short: "C", label: "Center" },
    { id: "right", short: "E", label: "Right" },
    { id: "bottom_left", short: "SW", label: "Bottom left" },
    { id: "bottom", short: "S", label: "Bottom" },
    { id: "bottom_right", short: "SE", label: "Bottom right" },
  ];

  function defaultRuleMap(): Record<TerrainRuleRole, string> {
    return Object.fromEntries(RULE_ROLES.map((role,index)=>[role.id,`${index%3},${Math.floor(index/3)}`])) as Record<TerrainRuleRole,string>;
  }

  let { workspaceId, worktreeId, assets, onError, onNotice }: {
    workspaceId: string; worktreeId: string; assets: Asset[];
    onError: (message: string) => void; onNotice: (message: string) => void;
  } = $props();

  let assetId = $state("");
  let name = $state("Terrain Tileset");
  let tileWidth = $state(16);
  let tileHeight = $state(16);
  let marginX = $state(0);
  let marginY = $state(0);
  let separationX = $state(0);
  let separationY = $state(0);
  let includeEmpty = $state(false);
  let autoConnect = $state(true);
  let terrainMode = $state<TerrainRuleMode>("blob_47");
  let terrainName = $state("Ground");
  let ruleMap = $state<Record<TerrainRuleRole,string>>(defaultRuleMap());
  let zoom = $state(2);
  let exporting = $state(false);
  let configuredAssetId = $state("");
  let loadedWorktreeId = $state("");
  let result = $state<TerrainExportResult>();

  const orderedAssets = $derived([...assets].sort((a,b)=>(a.category==="terrain"?0:1)-(b.category==="terrain"?0:1)||a.name.localeCompare(b.name)));
  const asset = $derived(orderedAssets.find(item=>item.id===assetId));
  const columns = $derived(axisCount(asset?.width??0,marginX,tileWidth,separationX));
  const rows = $derived(axisCount(asset?.height??0,marginY,tileHeight,separationY));
  const trailingX = $derived(axisTrailing(asset?.width??0,marginX,tileWidth,separationX,columns));
  const trailingY = $derived(axisTrailing(asset?.height??0,marginY,tileHeight,separationY,rows));
  const valid = $derived(Boolean(asset)&&tileWidth>0&&tileHeight>0&&marginX>=0&&marginY>=0&&separationX>=0&&separationY>=0&&columns>0&&rows>0);
  const ruleOptions = $derived(Array.from({length:columns*rows},(_,index)=>({value:`${index%columns},${Math.floor(index/columns)}`,label:`${index%columns+1}, ${Math.floor(index/columns)+1}`})));
  const selectedRuleValues = $derived(RULE_ROLES.map(role=>ruleMap[role.id]));
  const blobSizeValid = $derived(terrainMode!=="blob_47"||(tileWidth>=2&&tileHeight>=2&&tileWidth%2===0&&tileHeight%2===0));
  const terrainRulesValid = $derived(!autoConnect||(columns>=3&&rows>=3&&blobSizeValid&&selectedRuleValues.every(value=>ruleOptions.some(option=>option.value===value))&&new Set(selectedRuleValues).size===RULE_ROLES.length));
  const exportValid = $derived(valid&&terrainRulesValid);
  const selectedMarkers = $derived(autoConnect?RULE_ROLES.map(role=>{const [column,row]=ruleMap[role.id].split(",").map(Number);return {...role,column,row};}).filter(marker=>Number.isInteger(marker.column)&&Number.isInteger(marker.row)&&marker.column>=0&&marker.row>=0&&marker.column<columns&&marker.row<rows):[]);
  const gridWidth = $derived(columns?columns*tileWidth+Math.max(0,columns-1)*separationX:0);
  const gridHeight = $derived(rows?rows*tileHeight+Math.max(0,rows-1)*separationY:0);

  function axisCount(size:number,margin:number,tile:number,separation:number){
    if(size<=0||tile<=0||margin<0||separation<0||margin>=size||size-margin<tile)return 0;
    return 1+Math.floor((size-margin-tile)/(tile+separation));
  }
  function axisTrailing(size:number,margin:number,tile:number,separation:number,count:number){
    if(!count)return Math.max(0,size-margin);
    return size-margin-(count*tile+Math.max(0,count-1)*separation);
  }

  function usePreset(size:number){tileWidth=size;tileHeight=size;result=undefined;}
  function useFirstNine(){ruleMap=defaultRuleMap();result=undefined;}
  function useStudioRows(){
    ruleMap={top_left:"0,2",top:"0,1",top_right:"1,2",left:"3,1",center:"0,0",right:"1,1",bottom_left:"2,2",bottom:"2,1",bottom_right:"3,2"};
    result=undefined;
  }

  async function loadConfiguration(id:string){
    loadedWorktreeId=id;
    try{
      const saved=await api.getSetting(`terrain-export:${id}`) as Partial<{assetId:string;name:string;tileWidth:number;tileHeight:number;marginX:number;marginY:number;separationX:number;separationY:number;includeEmpty:boolean;autoConnect:boolean;terrainMode:TerrainRuleMode;terrainName:string;ruleMap:Record<TerrainRuleRole,string>;zoom:number}>|null;
      if(loadedWorktreeId!==id||!saved)return;
      if(saved.assetId){assetId=saved.assetId;configuredAssetId=saved.assetId;}
      if(saved.name)name=saved.name;
      if(saved.tileWidth)tileWidth=saved.tileWidth;
      if(saved.tileHeight)tileHeight=saved.tileHeight;
      marginX=Math.max(0,saved.marginX??marginX);marginY=Math.max(0,saved.marginY??marginY);
      separationX=Math.max(0,saved.separationX??separationX);separationY=Math.max(0,saved.separationY??separationY);
      includeEmpty=saved.includeEmpty??includeEmpty;autoConnect=saved.autoConnect??autoConnect;terrainMode=saved.terrainMode??terrainMode;terrainName=saved.terrainName??terrainName;
      if(saved.ruleMap)ruleMap={...defaultRuleMap(),...saved.ruleMap};
      zoom=saved.zoom??zoom;
    }catch{/* Export remains available even if preferences cannot be restored. */}
  }

  async function exportTileset(){
    if(!asset||!valid){onError("Choose an atlas and a tile size that fits inside it");return;}
    if(!terrainRulesValid){onError(!blobSizeValid?"Complete 47-tile generation requires even tile dimensions of at least 2 pixels":"Assign all nine terrain roles to different visible cells inside the atlas grid");return;}
    exporting=true;
    try{
      const terrainRules=autoConnect?RULE_ROLES.map(role=>{const [column,row]=ruleMap[role.id].split(",").map(Number);return {role:role.id,column,row};}):[];
      result=await api.exportGodotTileset({projectId:workspaceId,worktreeId,assetId:asset.id,name,tileWidth,tileHeight,marginX,marginY,separationX,separationY,includeEmpty,terrainName:autoConnect?terrainName:undefined,terrainMode:autoConnect?terrainMode:undefined,terrainRules});
      await api.setSetting(`terrain-export:${worktreeId}`,{assetId:asset.id,name,tileWidth,tileHeight,marginX,marginY,separationX,separationY,includeEmpty,autoConnect,terrainMode,terrainName,ruleMap,zoom});
      onNotice(autoConnect?`Godot TileSet exported with ${result.terrainRuleCount} auto-connect rules`:`Godot TileSet exported with ${result.occupiedTileCount} tiles`);
    }catch(error){onError(errorMessage(error));}
    finally{exporting=false;}
  }

  function revealResult(){if(result)void revealItemInDir(result.resourcePath);}

  $effect(()=>{
    if(worktreeId!==loadedWorktreeId)void loadConfiguration(worktreeId);
  });
  $effect(()=>{
    if(!orderedAssets.some(item=>item.id===assetId))assetId=orderedAssets[0]?.id??"";
    if(asset&&asset.id!==configuredAssetId){name=`${asset.name} Tileset`;configuredAssetId=asset.id;result=undefined;}
  });
</script>

<section class="terrain-studio">
  <header>
    <div><h1>Terrain slicer</h1><p>Slice an atlas and export a native Godot 4 TileSet resource.</p></div>
    <button class="primary" onclick={exportTileset} disabled={!exportValid||exporting}><Download size={14}/>{exporting?"Exporting…":"Export for Godot"}</button>
  </header>
  {#if orderedAssets.length}
    <div class="workspace">
      <aside>
        <label>Source atlas<select bind:value={assetId}>{#each orderedAssets as item}<option value={item.id}>{item.name} · {item.width}×{item.height}{item.category==="terrain"?" · terrain":""}</option>{/each}</select></label>
        <label>Export name<input bind:value={name}/></label>
        <div class="presets"><span>Tile presets</span>{#each [16,32,48,64] as size}<button class:active={tileWidth===size&&tileHeight===size} onclick={()=>usePreset(size)}>{size}</button>{/each}</div>
        <div class="pair"><label>Tile width<input type="number" min="1" max="4096" bind:value={tileWidth}/></label><label>Tile height<input type="number" min="1" max="4096" bind:value={tileHeight}/></label></div>
        <h2>Atlas guides</h2>
        <div class="pair"><label>Margin X<input type="number" min="0" max="1024" bind:value={marginX}/></label><label>Margin Y<input type="number" min="0" max="1024" bind:value={marginY}/></label></div>
        <div class="pair"><label>Separation X<input type="number" min="0" max="1024" bind:value={separationX}/></label><label>Separation Y<input type="number" min="0" max="1024" bind:value={separationY}/></label></div>
        <label class="check"><input type="checkbox" bind:checked={includeEmpty}/> Include fully transparent cells</label>
        <div class="validation" class:bad={!valid}>
          {#if valid}<CheckCircle2 size={15}/><div><strong>{columns} × {rows} grid · {columns*rows} cells</strong><span>{trailingX||trailingY?`${trailingX}px right / ${trailingY}px bottom left outside the grid`:"Atlas fits the grid exactly"}</span></div>{:else}<AlertTriangle size={15}/><div><strong>Grid does not fit</strong><span>Reduce the tile size or margins.</span></div>{/if}
        </div>
        <h2>Terrain auto-connect</h2>
        <label class="check"><input type="checkbox" bind:checked={autoConnect}/> Add Godot terrain rules</label>
        {#if autoConnect}
          <label>Rule set<select bind:value={terrainMode}><option value="blob_47">Complete 47-tile blob</option><option value="nine_slice">9-tile starter</option></select></label>
          <label>Terrain name<input bind:value={terrainName} placeholder="Ground"/></label>
          <div class="rule-heading"><span>Source role → atlas cell</span><div><button onclick={useStudioRows} disabled={columns<4||rows<3}>Studio rows</button><button onclick={useFirstNine}>First 3×3</button></div></div>
          <div class="rule-grid">
            {#each RULE_ROLES as role}
              <label title={role.label}><span>{role.short}</span><select bind:value={ruleMap[role.id]}>{#each ruleOptions as option}<option value={option.value}>{option.label}</option>{/each}</select></label>
            {/each}
          </div>
          <div class="rule-status" class:bad={!terrainRulesValid}>
            {#if terrainRulesValid}<CheckCircle2 size={14}/><span>{terrainMode==="blob_47"?"Nine source roles ready → 47 canonical masks":"Nine unique rules ready"}</span>{:else}<AlertTriangle size={14}/><span>{!blobSizeValid?"47-tile mode needs even tile dimensions":"Needs a 3×3 grid and nine unique cells"}</span>{/if}
          </div>
          <p class="coverage-note">{terrainMode==="blob_47"?"Generates a new 8×6 atlas containing all 47 canonical masks; the last cell stays transparent. Covers concave corners, islands, and one-cell channels.":"Covers solid-region edges and outer corners only. Use the complete set for irregular terrain."}</p>
        {/if}
        <div class="godot-note"><strong>Godot path</strong><p>The generated resource points to <code>res://exports/godot/…</code>, so this Sprite Studio project folder should also be the Godot project root.</p></div>
      </aside>
      <main>
        <div class="toolbar"><div><Map size={13}/><strong>Atlas preview</strong><span>{asset?.width}×{asset?.height}</span></div><label>Zoom<select bind:value={zoom}><option value={1}>1×</option><option value={2}>2×</option><option value={4}>4×</option><option value={8}>8×</option></select></label></div>
        <div class="stage">
          {#if asset}
            <div class="atlas" style={`width:${asset.width*zoom}px;height:${asset.height*zoom}px`}>
              <img src={assetUrl(asset.path)} alt={asset.name} style={`width:${asset.width*zoom}px;height:${asset.height*zoom}px`}/>
              {#if valid}<div class="grid" style={`left:${marginX*zoom}px;top:${marginY*zoom}px;width:${gridWidth*zoom}px;height:${gridHeight*zoom}px;background-size:${(tileWidth+separationX)*zoom}px ${(tileHeight+separationY)*zoom}px;--cell-w:${tileWidth*zoom}px;--cell-h:${tileHeight*zoom}px`}></div>{/if}
              {#each selectedMarkers as marker}<span class="rule-marker" title={marker.label} style={`left:${(marginX+marker.column*(tileWidth+separationX))*zoom+3}px;top:${(marginY+marker.row*(tileHeight+separationY))*zoom+3}px`}>{marker.short}</span>{/each}
            </div>
          {/if}
        </div>
        <footer>
          {#if result}<div class="result"><span class="success"><CheckCircle2 size={15}/></span><div><strong>{result.occupiedTileCount} cells exported{result.terrainRuleCount?` · ${result.terrainRuleCount} auto-connect rules`:""}</strong><span>{result.terrainMode==="blob_47"?"Complete 47-tile atlas · ":""}{result.resourcePath}</span></div><button onclick={revealResult}><FolderOpen size={13}/>Reveal</button></div>{:else}<div class="hint"><Grid3X3 size={15}/><span>The grid is non-destructive. Export copies the atlas and leaves the source asset untouched.</span></div>{/if}
        </footer>
      </main>
    </div>
  {:else}<div class="empty"><Map size={30}/><strong>No terrain atlas linked to this project section</strong><p>Import or link a terrain image from the Sprites tab, then return here to slice it.</p></div>{/if}
</section>

<style>
  .terrain-studio{height:100%;display:flex;flex-direction:column;background:var(--bg)}header{height:58px;min-height:58px;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;padding:0 16px}h1{font-size:14px;margin:0}header p{font-size:10px;color:var(--faint);margin:4px 0 0}.primary{height:31px;border:1px solid var(--text);background:var(--text);color:var(--bg);border-radius:6px;padding:0 10px;display:flex;align-items:center;gap:6px;font:inherit;font-size:11px;cursor:pointer}.primary:disabled{opacity:.5}.workspace{min-height:0;flex:1;display:grid;grid-template-columns:330px minmax(0,1fr)}aside{min-height:0;overflow:auto;border-right:1px solid var(--border);background:var(--sidebar);padding:14px}aside>label,.pair label{display:block;font-size:10px;color:var(--muted);margin-bottom:11px}aside input,aside select,.toolbar select{display:block;width:100%;height:29px;margin-top:5px;border:1px solid var(--border);border-radius:5px;background:var(--bg);color:var(--text);font:inherit;font-size:11px;padding:0 7px}.pair{display:grid;grid-template-columns:1fr 1fr;gap:7px}aside h2{font-size:9px;letter-spacing:.12em;text-transform:uppercase;color:var(--faint);margin:15px 0 10px}.presets{display:flex;align-items:center;gap:5px;margin:0 0 12px}.presets span{font-size:9px;color:var(--faint);margin-right:auto}.presets button{height:25px;min-width:31px;border:1px solid var(--border);border-radius:4px;background:var(--surface);color:var(--muted);font:inherit;font-size:9px;cursor:pointer}.presets button.active{border-color:var(--accent);color:var(--text);background:var(--accent-dim)}.check{display:flex!important;align-items:center;gap:7px;margin:4px 0 13px!important}.check input{width:14px;height:14px;margin:0}.validation{display:flex;gap:8px;padding:9px;border:1px solid #4f8b6f55;background:#4f8b6f12;border-radius:6px;color:#72b493}.validation.bad{border-color:#cf777255;background:#cf777212;color:#cf7772}.validation strong,.validation span{display:block}.validation strong{font-size:10px;color:var(--text)}.validation span{font-size:9px;color:var(--faint);margin-top:4px;line-height:1.35}.rule-heading{display:flex;align-items:center;justify-content:space-between;margin:-2px 0 7px;font-size:9px;color:var(--faint)}.rule-heading>div{display:flex;gap:4px}.rule-heading button{height:23px;border:1px solid var(--border);border-radius:4px;background:var(--surface);color:var(--muted);font:inherit;font-size:9px;cursor:pointer}.rule-heading button:disabled{opacity:.45;cursor:default}.rule-grid{display:grid;grid-template-columns:repeat(3,1fr);gap:5px}.rule-grid label{display:grid;grid-template-columns:24px 1fr;align-items:center;border:1px solid var(--border);border-radius:5px;background:var(--surface);padding-left:6px}.rule-grid label>span{font-size:9px;font-weight:700;color:var(--accent)}.rule-grid select{height:27px;margin:0;border:0;border-left:1px solid var(--border);border-radius:0 4px 4px 0;background:var(--bg);font-size:9px}.rule-status{display:flex;align-items:center;gap:6px;margin-top:8px;color:#72b493;font-size:9px}.rule-status.bad{color:#cf7772}.coverage-note{font-size:9px;line-height:1.45;color:var(--faint);margin:8px 0 0}.godot-note{border-top:1px solid var(--border);margin-top:13px;padding-top:12px}.godot-note strong{font-size:10px}.godot-note p{font-size:9px;line-height:1.45;color:var(--faint);margin:5px 0}.godot-note code{color:var(--muted)}main{min-width:0;min-height:0;display:grid;grid-template-rows:42px minmax(0,1fr) 58px}.toolbar{border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;padding:0 12px}.toolbar>div{display:flex;align-items:center;gap:7px}.toolbar strong{font-size:10px}.toolbar span{font-size:9px;color:var(--faint)}.toolbar label{display:flex;align-items:center;gap:6px;font-size:9px;color:var(--faint)}.toolbar select{width:58px;height:25px;margin:0}.stage{min-height:0;overflow:auto;padding:30px;display:grid;place-items:center;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:16px 16px;background-position:0 0,0 8px,8px -8px,-8px 0}.atlas{position:relative;box-shadow:0 0 0 1px var(--border-strong),0 8px 28px #0005}.atlas img{display:block;max-width:none;image-rendering:pixelated}.grid{position:absolute;pointer-events:none;border:1px solid #f6c85d;background-image:linear-gradient(to right,#f6c85d 1px,transparent 1px),linear-gradient(to bottom,#f6c85d 1px,transparent 1px),linear-gradient(to right,transparent var(--cell-w),#f6c85d var(--cell-w),#f6c85d calc(var(--cell-w) + 1px),transparent calc(var(--cell-w) + 1px)),linear-gradient(to bottom,transparent var(--cell-h),#f6c85d var(--cell-h),#f6c85d calc(var(--cell-h) + 1px),transparent calc(var(--cell-h) + 1px));box-sizing:border-box;filter:drop-shadow(0 0 1px #000)}.rule-marker{position:absolute;z-index:2;pointer-events:none;min-width:19px;height:17px;padding:0 3px;border:1px solid #fff6;border-radius:4px;background:#111d;color:#fff;display:grid;place-items:center;font-size:8px;font-weight:800;line-height:1}footer{border-top:1px solid var(--border);background:var(--sidebar);display:flex;align-items:center;padding:0 13px}.result,.hint{width:100%;display:flex;align-items:center;gap:8px}.success{color:#72b493}.result div{min-width:0}.result strong,.result span{display:block}.result strong{font-size:10px}.result span{font-size:9px;color:var(--faint);margin-top:3px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.result button{margin-left:auto;height:27px;border:1px solid var(--border);border-radius:5px;background:var(--surface);color:var(--muted);font:inherit;font-size:10px;display:flex;align-items:center;gap:5px;padding:0 8px;cursor:pointer}.hint{color:var(--faint);font-size:9px}.empty{flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;color:var(--faint);text-align:center}.empty strong{font-size:12px;color:var(--text);margin-top:10px}.empty p{font-size:10px;margin:5px 0}@media(max-width:1000px){.workspace{grid-template-columns:280px minmax(0,1fr)}}
</style>
