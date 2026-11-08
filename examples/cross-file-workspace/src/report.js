const { add } = require("./math.js");

function report(a, b) {
  return add(a, b);
}

module.exports = { report };
