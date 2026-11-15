// The part registry.
//
// Every component is a file under `kit/parts/` that registers itself here.
// A scene loads only the components it assembles, so a film does not carry
// the drawing code of a scene it never uses.
//
// A component is a function that returns:
//
//   {
//     bones: [...],   // parent before child, solved together with the scene
//     parts: [...],   // drawn in their bone's frame
//     z: 0,           // paint order, low first
//     update(t, dt),  // optional, sets bone angles and steps springs
//   }
(function () {
  const PARTS = {};
  const pending = {};

  PARTS.load = function (src) {
    if (pending[src]) return pending[src];
    pending[src] = new Promise(function (resolve, reject) {
      const script = document.createElement("script");
      script.src = src;
      script.onload = function () {
        resolve();
      };
      script.onerror = function () {
        reject(new Error("cannot load " + src));
      };
      document.head.appendChild(script);
    });
    return pending[src];
  };

  window.PARTS = PARTS;
})();
