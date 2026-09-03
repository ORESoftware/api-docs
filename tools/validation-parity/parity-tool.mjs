#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

const root = process.cwd();
const mode = process.argv.find((x) => /^--(?:check|write|self-test)$/.test(x));
if (!mode) throw new Error('usage: parity-tool.mjs --check|--write|--self-test');
const hash = (s) => createHash('sha256').update(s).digest('hex');
const sort = (v) => Array.isArray(v) ? v.map(sort) : v && typeof v === 'object'
  ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, sort(v[k])])) : v;
const json = (v) => JSON.stringify(sort(v), null, 2) + '\n';
const read = (p) => readFileSync(resolve(root, p), 'utf8');
const readJson = (p) => JSON.parse(read(p));
const ok = (v, m) => { if (!v) throw new Error(m); };

function scalarFromJson(x) {
  if (x.$ref) return { kind: 'ref', ref: x.$ref.split('/').at(-1) };
  if (Array.isArray(x.type)) {
    const nonNull = x.type.filter((v) => v !== 'null');
    ok(nonNull.length === 1, `unsupported JSON Schema union ${JSON.stringify(x.type)}`);
    return { ...scalarFromJson({ ...x, type: nonNull[0] }), nullable: x.type.includes('null') };
  }
  if (x.type === 'array') return { kind: 'array', items: scalarFromJson(x.items) };
  ok(['string', 'integer', 'number', 'boolean'].includes(x.type), `unsupported JSON Schema type ${x.type}`);
  const out = { kind: x.type };
  for (const k of ['minLength','maxLength','minimum','maximum','exclusiveMinimum','exclusiveMaximum','pattern','format','default'])
    if (Object.hasOwn(x, k)) out[k] = x[k];
  if (x.enum) out.enum = [...x.enum];
  return out;
}
function jsonIr(doc, names) {
  ok(doc.$defs, 'JSON Schema must have $defs');
  const models = {};
  for (const name of [...names].sort()) {
    const m = doc.$defs[name];
    ok(m?.type === 'object', `JSON Schema missing object model ${name}`);
    const req = new Set(m.required ?? []), fields = {};
    for (const key of Object.keys(m.properties ?? {}).sort())
      fields[key] = { required: req.has(key), ...scalarFromJson(m.properties[key]) };
    models[name] = { closed: m.additionalProperties === false, fields };
  }
  return { models };
}
function modelBlocks(src) {
  src = src.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '').replace(/@doc\s*\([^\n]*\)\s*/g, '');
  const out = [], re = /\bmodel\s+(\w+)\s*\{/g;
  let m;
  while ((m = re.exec(src))) {
    let i = re.lastIndex, depth = 1, q = '', esc = false;
    for (; i < src.length && depth; i++) {
      const c = src[i];
      if (q) { if (esc) esc = false; else if (c === '\\') esc = true; else if (c === q) q = ''; }
      else if (c === '"' || c === "'") q = c;
      else if (c === '{') depth++; else if (c === '}') depth--;
    }
    ok(!depth, `unclosed TypeSpec model ${m[1]}`);
    out.push([m[1], src.slice(re.lastIndex, i - 1)]); re.lastIndex = i;
  }
  return out;
}
function statements(body) {
  const out = []; let start = 0, p = 0, b = 0, q = '', esc = false;
  for (let i = 0; i < body.length; i++) {
    const c = body[i];
    if (q) { if (esc) esc = false; else if (c === '\\') esc = true; else if (c === q) q = ''; continue; }
    if (c === '"' || c === "'") { q = c; continue; }
    if (c === '(' || c === '<' || c === '[') p++; else if (c === ')' || c === '>' || c === ']') p--;
    else if (c === ';' && !p) { const x = body.slice(start, i).trim(); if (x) out.push(x); start = i + 1; }
  }
  return out;
}
function decorators(s) {
  const out = {}, map = { minLength:'minLength', maxLength:'maxLength', minValue:'minimum', maxValue:'maximum', minValueExclusive:'exclusiveMinimum', maxValueExclusive:'exclusiveMaximum' };
  for (const m of s.matchAll(/@(\w+)\s*\(([^)]*)\)/g)) {
    if (map[m[1]]) out[map[m[1]]] = Number(m[2]);
    else if (m[1] === 'pattern' || m[1] === 'format') out[m[1]] = JSON.parse(m[2]);
  }
  return out;
}
function scalarFromTsp(raw) {
  raw = raw.trim();
  if (raw.endsWith('[]')) return { kind:'array', items:scalarFromTsp(raw.slice(0,-2)) };
  const a = raw.match(/^Array\s*<(.+)>$/); if (a) return { kind:'array', items:scalarFromTsp(a[1]) };
  const union = raw.split('|').map((x) => x.trim());
  if (union.includes('null')) { const n = union.filter((x) => x !== 'null'); ok(n.length === 1, `unsupported TypeSpec union ${raw}`); return { ...scalarFromTsp(n[0]), nullable:true }; }
  if (['string','url','plainDate','utcDateTime','offsetDateTime'].includes(raw)) return { kind:'string', ...(raw === 'url' ? {format:'uri'} : {}) };
  if (/^(?:u?int(?:8|16|32|64)?|safeint)$/.test(raw)) return { kind:'integer' };
  if (/^(?:float(?:32|64)?|decimal(?:128)?)$/.test(raw)) return { kind:'number' };
  if (raw === 'boolean') return { kind:'boolean' };
  ok(/^\w+(?:\.\w+)*$/.test(raw), `unsupported TypeSpec type ${raw}`); return { kind:'ref', ref:raw.split('.').at(-1) };
}
function literal(x) { x=x.trim(); if (/^-?\d+(?:\.\d+)?$/.test(x)) return Number(x); if (/^(?:true|false|null)$/.test(x)) return JSON.parse(x); if (x.startsWith('"')) return JSON.parse(x); return x; }
function tspIr(src, names) {
  const wanted = new Set(names), models = {};
  for (const [name, body] of modelBlocks(src)) {
    if (!wanted.has(name)) continue;
    const fields = {};
    for (const s of statements(body)) {
      const m = s.match(/^([\s\S]*?)\b(\w+)(\?)?\s*:\s*([^=]+?)(?:\s*=\s*([\s\S]+))?$/);
      ok(m, `unsupported TypeSpec field in ${name}: ${s}`);
      fields[m[2]] = { required: !m[3], ...scalarFromTsp(m[4]), ...decorators(m[1]), ...(m[5] ? {default:literal(m[5])} : {}) };
    }
    models[name] = { closed:true, fields };
  }
  for (const name of wanted) ok(models[name], `TypeSpec missing model ${name}`);
  return sort({ models });
}
function diff(a,b,p='$') {
  if (Object.is(a,b)) return [];
  if (!a || !b || typeof a !== typeof b) return [`${p}: ${JSON.stringify(a)} != ${JSON.stringify(b)}`];
  if (Array.isArray(a) || Array.isArray(b)) return JSON.stringify(a) === JSON.stringify(b) ? [] : [`${p}: ${JSON.stringify(a)} != ${JSON.stringify(b)}`];
  if (typeof a !== 'object') return [`${p}: ${JSON.stringify(a)} != ${JSON.stringify(b)}`];
  return [...new Set([...Object.keys(a),...Object.keys(b)])].sort().flatMap((k) => !Object.hasOwn(a,k) || !Object.hasOwn(b,k) ? [`${p}.${k}: missing`] : diff(a[k],b[k],`${p}.${k}`));
}
const snake = (s) => s.replace(/([a-z0-9])([A-Z])/g,'$1_$2').replace(/\W/g,'_').toLowerCase();
const pascal = (s) => s.replace(/(^|[_\-.])(\w)/g,(_,__,c)=>c.toUpperCase());
function type(f, lang) {
  const base = f.kind === 'string' ? ({ts:'string',rs:'String',go:'string',gleam:'String'}[lang]) : f.kind === 'integer' ? ({ts:'number',rs:'i64',go:'int64',gleam:'Int'}[lang]) : f.kind === 'number' ? ({ts:'number',rs:'f64',go:'float64',gleam:'Float'}[lang]) : f.kind === 'boolean' ? ({ts:'boolean',rs:'bool',go:'bool',gleam:'Bool'}[lang]) : f.kind === 'ref' ? f.ref : f.kind === 'array' ? ({ts:`ReadonlyArray<${type({...f.items,required:true},lang)}>`,rs:`Vec<${type({...f.items,required:true},lang)}>`,go:`[]${type({...f.items,required:true},lang)}`,gleam:`List(${type({...f.items,required:true},lang)})`}[lang]) : (()=>{throw new Error(`unsupported ${f.kind}`)})();
  if (f.required || lang === 'ts') return base; return {rs:`Option<${base}>`,go:`*${base}`,gleam:`Option(${base})`}[lang];
}
function targets(ir, cfg, scope) {
  const b='// Generated only after independent JSON Schema and TypeSpec agreement. DO NOT EDIT.\n', ms=Object.entries(sort(ir).models);
  const ts=`${b}\nexport const contractVersion=${JSON.stringify(cfg.contractVersion)} as const;\nexport const contractScope=${JSON.stringify(scope)} as const;\n\n${ms.map(([n,m])=>`export interface ${n} {\n${Object.entries(m.fields).map(([k,f])=>`  readonly ${k}${f.required?'':'?'}: ${type(f,'ts')};`).join('\n')}\n}`).join('\n\n')}\n`;
  const rs=`//!${b.slice(2)}\n${ms.map(([n,m])=>`#[derive(Clone, Debug, PartialEq)]\npub struct ${n} {\n${Object.entries(m.fields).map(([k,f])=>`    pub ${snake(k)}: ${type(f,'rs')},`).join('\n')}\n}`).join('\n\n')}\n`;
  const go=`${b}\npackage ${(cfg.codegen?.goPackage??'interfaces')}_${snake(scope)}\n\nconst ContractVersion=${JSON.stringify(cfg.contractVersion)}\n\n${ms.map(([n,m])=>`type ${n} struct {\n${Object.entries(m.fields).map(([k,f])=>`\t${pascal(k)} ${type(f,'go')} \`json:"${k}${f.required?'':',omitempty'}"\``).join('\n')}\n}`).join('\n\n')}\n`;
  const module=`${cfg.codegen?.gleamModule??'validation_interfaces'}_${snake(scope)}`;
  const gleam=`${b}import gleam/option.{type Option}\n\npub const contract_version=${JSON.stringify(cfg.contractVersion)}\n\n${ms.map(([n,m])=>`pub type ${n} {\n  ${n}(\n${Object.entries(m.fields).map(([k,f])=>`    ${snake(k)}: ${type(f,'gleam')},`).join('\n')}\n  )\n}`).join('\n\n')}\n`;
  return { [`typescript/${scope}/types.ts`]:ts, [`rust/${scope}/src/lib.rs`]:rs, [`golang/${scope}/types.go`]:go, [`gleam/${scope}/src/${module}.gleam`]:gleam };
}
function build() {
  const cfg=readJson('validation/parity/manifest.v2.json'), out=cfg.outputRoot??'generated/final', cand=cfg.candidatesRoot??'generated/candidates', files={}, receipts={}, active=[], assigned=new Map();
  for (const [scope,s] of Object.entries(cfg.scopes)) for (const model of s.models??[]) { ok(!assigned.has(model), `model ${model} assigned to both ${assigned.get(model)} and ${scope}`); assigned.set(model,scope); }
  for (const [scope,s] of Object.entries(cfg.scopes)) {
    ok(['isomorphic','client','edge','server'].includes(scope),`unknown scope ${scope}`); if (!s.models.length) continue;
    const auth=s.authorities??cfg.authorities, a=jsonIr(readJson(auth.jsonSchema),s.models), b=tspIr(read(auth.typespec),s.models), d=diff(a,b);
    ok(!d.length,`authority disagreement in ${scope}; final definitions not written:\n${d.slice(0,100).join('\n')}`);
    if (scope!=='server') for (const x of cfg.forbiddenPublicModels??[]) ok(!JSON.stringify(a).includes(`"${x}"`),`server model leaked: ${x}`);
    const ta=targets(a,cfg,scope), tb=targets(b,cfg,scope); ok(!diff(ta,tb).length,`independent generators disagree in ${scope}`);
    files[`${cand}/json-schema/${scope}.signature.json`]=json(a); files[`${cand}/typespec/${scope}.signature.json`]=json(b);
    for (const [p,c] of Object.entries(ta)) files[`${out}/${p}`]=c;
    receipts[scope]={models:[...s.models].sort(),authorities:{jsonSchema:{path:auth.jsonSchema,semanticDigest:hash(json(a))},typespec:{path:auth.typespec,semanticDigest:hash(json(b))}},targetDigests:Object.fromEntries(Object.entries(ta).sort().map(([p,c])=>[p,hash(c)])),agreement:true}; active.push(scope);
  }
  for (const [runtime,scopes] of Object.entries(cfg.runtimeExports)) {
    const exports=scopes.filter((x)=>active.includes(x)); if (['browser','edge'].includes(runtime)) ok(!exports.includes('server'),`${runtime} cannot export server models`);
    const lines=exports.flatMap((scope)=>[
      `export type { ${receipts[scope].models.join(', ')} } from "../../${scope}/types.js";`,
      `export { contractVersion as ${scope}ContractVersion, contractScope as ${scope}ContractScope } from "../../${scope}/types.js";`,
    ]);
    files[`${out}/typescript/runtime/${runtime}/index.ts`]=`// Generated after parity. DO NOT EDIT.\n${lines.join('\n')}\nexport const validationRuntime=${JSON.stringify(runtime)} as const;\n`;
  }
  let routeBindings=null;
  if (cfg.routeBindings) { const doc=readJson(cfg.routeBindings), ids=(doc.bindings??[]).map((x)=>{ok(x.operationId?.trim(),'route binding needs api-docs operationId');return x.operationId.trim()}); ok(new Set(ids).size===ids.length,'duplicate operationId'); routeBindings={path:cfg.routeBindings,semanticDigest:hash(json(doc)),count:ids.length,operationIds:ids.sort()}; }
  const digests=Object.fromEntries(Object.entries(files).filter(([p])=>p.startsWith(out+'/')).sort().map(([p,c])=>[p.slice(out.length+1),hash(c)]));
  const receipt={receiptVersion:'ores.validation.parity-receipt.v2',contractVersion:cfg.contractVersion,repository:cfg.repository,scopes:receipts,runtimeExports:cfg.runtimeExports,agreement:Object.values(receipts).every((x)=>x.agreement),finalTargetDigests:digests,finalAggregateDigest:hash(json(digests)),routeAuthority:cfg.routeAuthority,routeBindings,generatedOnlyAfterAgreement:true};
  files[cfg.receipt??`${out}/parity-receipt.v2.json`]=json(receipt); return {files,receipt};
}
function selfTest(){const j={$defs:{X:{type:'object',additionalProperties:false,required:['id'],properties:{id:{type:'string',minLength:1}}}}},t='model X { @minLength(1) id: string; }',a=jsonIr(j,['X']);ok(!diff(a,tspIr(t,['X'])).length,'equivalent inputs differ');ok(diff(a,tspIr(t.replace('id:','id?:'),['X'])).length,'requiredness drift missed');console.log('parity self-tests passed');}
if(mode==='--self-test') selfTest(); else {const {files,receipt}=build(), failures=[]; for(const [p,c] of Object.entries(files)){const f=resolve(root,p); if(mode==='--write'){mkdirSync(dirname(f),{recursive:true});writeFileSync(f,c);} else if(!existsSync(f)||readFileSync(f,'utf8')!==c) failures.push(p);} ok(!failures.length,`missing/stale generated files:\n${failures.join('\n')}`);console.log(`${mode==='--write'?'wrote':'verified'} ${Object.keys(files).length} files; digest=${receipt.finalAggregateDigest}`);}
