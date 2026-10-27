const ready = require("./ready");
if (!ready) {
  return;
}

module.exports = function greet(name) {
  return 'hello, ' + name;
};
