// A figure cut from a traced reference.
//
// The pieces arrive already cut: `head` is the picture above the traced neck,
// `body` is the picture below it, both with the silhouette baked in. This
// component only places them and gives them bones — one at the neck, one at
// the base — so the head turns on a joint instead of a guessed ellipse.
//
// There is no drawing code for a face here, on purpose.
(function () {
  const R = window.RIG;

  /** Cover-fits an image into a plate of `width` × `height`. */
  window.PARTS.fitPlate = function (image, width, height) {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    const scale = Math.max(width / image.width, height / image.height);
    const drawnWidth = image.width * scale;
    const drawnHeight = image.height * scale;
    ctx.drawImage(
      image,
      (width - drawnWidth) / 2,
      (height - drawnHeight) / 2,
      drawnWidth,
      drawnHeight,
    );
    return canvas;
  };

  window.PARTS.portrait = function (opts) {
    const o = Object.assign({ x: 580, y: 40 }, opts);
    const headPiece = o.head;
    const bodyPiece = o.body;
    const neck = o.neck;
    if (!headPiece || !bodyPiece || !neck) {
      throw new Error("the portrait part needs traced head and body pieces");
    }
    const W = headPiece.width;
    const H = headPiece.height;

    // The head pivots at the neck; the body breathes from its base.
    const head = R.bone(null, o.x + neck.x, o.y + neck.y, 0);
    const body = R.bone(null, o.x + W / 2, o.y + H, 0);
    const headRest = head.y;
    const lag = { value: 0, velocity: 0 };

    const headPart = R.part({
      bone: head,
      draw: function (ctx) {
        ctx.drawImage(headPiece, -neck.x, -neck.y);
      },
    });

    const bodyPart = R.part({
      bone: body,
      draw: function (ctx) {
        ctx.drawImage(bodyPiece, -W / 2, -H);
      },
    });

    return {
      bones: [head, body],
      parts: [bodyPart, headPart],
      z: 100,
      update: function (t, dt) {
        const base = Math.sin(t * 0.5) * 0.025 + Math.sin(t * 0.23 + 0.7) * 0.01;
        head.angle = base;
        head.y = headRest + Math.sin(t * 0.9) * 2.2;
        body.angle = Math.sin(t * 0.4) * 0.0025;
        // The head settles after the turn; without it a cut-out looks pasted.
        const settle = R.springValue(lag, base * 0.6, 40, 9, dt || 1 / 60);
        head.angle += settle * 0.2;
      },
    };
  };
})();
