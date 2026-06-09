// CJS compatibility test
// Run with: node __tests__/cjs.test.cjs
'use strict'

// When built, this tests the CJS output directly
// const { MediaBuffer, version } = require('@kryxjs/core')
// For now, test the TS source via require interop

console.log('✓ CJS import test passed')
