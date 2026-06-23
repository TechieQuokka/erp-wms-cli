// Generates medium-volume 3PL CSV fixtures for the CLI scenario suite, matching
// the backend import grammars:
//   items:      sku,name,barcode,uom,min_level
//   locations:  code,zone,type,capacity
//   orders:     order_ref,sku,qty,ship_to_name,ship_to_address,ship_to_phone
//
//   node gen-fixtures.mjs <outDir> <runId>
//
// Also emits stock.tsv (tenant<TAB>sku<TAB>location<TAB>qty) — the SKUs the runner
// stocks via inbound putaway — and tenants.txt / a few *.first files the runner reads.

import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const [outDir, runId = String(Date.now())] = process.argv.slice(2)
if (!outDir) {
  console.error('usage: node gen-fixtures.mjs <outDir> <runId>')
  process.exit(2)
}
mkdirSync(outDir, { recursive: true })

const N_LOC = Number(process.env.N_LOC || 50) // global locations
const N_ITEMS = Number(process.env.N_ITEMS || 50) // items per tenant
const N_STOCK = Number(process.env.N_STOCK || 20) // SKUs per tenant that get inbound stock
const N_ORDERS = Number(process.env.N_ORDERS || 50) // orders per tenant
const TENANTS = (process.env.TENANTS || 'ACME,GLOBEX').split(',')
const SFX = runId.slice(-5) // keep codes short but run-unique

const pad = (n, w = 4) => String(n).padStart(w, '0')

// --- locations (warehouse-global) ------------------------------------------
const zones = ['A', 'B', 'C', 'D']
const types = ['storage', 'storage', 'picking', 'storage', 'receiving', 'shipping']
const locCodes = []
let locCsv = 'code,zone,type,capacity\n'
for (let i = 1; i <= N_LOC; i++) {
  const code = `${zones[i % zones.length]}-${pad(i, 2)}-${SFX}`
  locCodes.push(code)
  locCsv += `${code},${zones[i % zones.length]},${types[i % types.length]},100\n`
}
writeFileSync(join(outDir, 'locations.csv'), locCsv)

// --- per-tenant items + orders, plus a stock manifest ----------------------
const stockLines = []
const firstStockSku = {} // tenant -> first stocked sku (for targeted assertions)
for (const T of TENANTS) {
  const skus = []
  let itemsCsv = 'sku,name,barcode,uom,min_level\n'
  for (let i = 1; i <= N_ITEMS; i++) {
    const sku = `${T}-SKU-${pad(i)}`
    skus.push(sku)
    // A handful carry a min level → they surface as low-stock alerts (they are
    // NOT stocked below, so on-hand 0 < min_level).
    const min = i > N_STOCK && i <= N_STOCK + 5 ? 10 : 0
    itemsCsv += `${sku},${T} Product ${i},BC${T}${pad(i)},ea,${min}\n`
  }
  writeFileSync(join(outDir, `items-${T}.csv`), itemsCsv)
  firstStockSku[T] = skus[0]

  // Stock the first N_STOCK SKUs (qty 100 each) into rotating locations.
  for (let i = 0; i < N_STOCK; i++) {
    stockLines.push([T, skus[i], locCodes[i % locCodes.length], '100'].join('\t'))
  }

  // Orders reference stocked SKUs (so they allocate), 1-3 lines each.
  let ordersCsv = 'order_ref,sku,qty,ship_to_name,ship_to_address,ship_to_phone\n'
  for (let o = 1; o <= N_ORDERS; o++) {
    const ref = `${T}-ORD-${pad(o)}`
    const nLines = 1 + (o % 3)
    for (let l = 0; l < nLines; l++) {
      const sku = skus[(o + l) % N_STOCK]
      const qty = 1 + ((o + l) % 4)
      ordersCsv += `${ref},${sku},${qty},Customer ${o},${o} Main St,555-${pad(o)}\n`
    }
  }
  writeFileSync(join(outDir, `orders-${T}.csv`), ordersCsv)
}

writeFileSync(join(outDir, 'tenants.txt'), TENANTS.join('\n') + '\n')
writeFileSync(join(outDir, 'stock.tsv'), stockLines.join('\n') + '\n')
writeFileSync(join(outDir, 'first-stock-sku.json'), JSON.stringify(firstStockSku))

console.error(
  `fixtures: ${N_LOC} locations, ${N_ITEMS}x${TENANTS.length} items, ` +
    `${stockLines.length} stock lines, ${N_ORDERS}x${TENANTS.length} orders (suffix ${SFX})`,
)
