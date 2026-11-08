const assert = require("node:assert");
const { report } = require("./src/report.js");

assert.strictEqual(report(2, 3), 5);
console.log("ok");
