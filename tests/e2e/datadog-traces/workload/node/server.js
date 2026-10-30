// Pricing service: a child span per request and a server error for one order.
const tracer = require('dd-trace')
const express = require('express')

const app = express()
app.get('/price/:order', (req, res) => {
  tracer.trace('price.lookup', { resource: 'pricing-table' }, (span) => {
    span.setTag('price.currency', 'USD')
  })
  if (req.params.order === '5') {
    return res.status(500).json({ error: 'pricing unavailable' })
  }
  res.json({ order: req.params.order, price: 9.99 })
})
const server = app.listen(3000)
// Close the server on `docker stop`; dd-trace flushes once the event loop drains.
process.on('SIGTERM', () => server.close())
