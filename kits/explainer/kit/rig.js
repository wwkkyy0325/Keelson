// Bones, springs and parts: the rig behind a cut-out figure.
//
// A scene assembles its components from parts. Each part is drawn in the
// frame of one bone, so moving the bone moves the part; a bone moves because
// its parent moved, because a keyframe drove it, or because an IK solve put
// its tip on a target.
//
// A spring is a chain of points whose first point follows a bone and whose
// rest lag behind. Hair, cloth, rigging and foam are springs: they arrive
// after the body and overshoot it, which is what makes a still drawing move
// like a drawing rather than like a slide.
//
// Everything here is deterministic in `t` once `reset()` has run, so a
// rendered frame can be reproduced.
(function () {
  const R = {};

  /* ---------- bones ---------- */

  /**
   * A bone: an offset and an angle in its parent's frame. `length` is only
   * used by IK and by parts that draw a limb from the joint to the tip.
   */
  R.bone = function (parent, x, y, length) {
    const bone = {
      parent: parent || null,
      children: [],
      x: x || 0,
      y: y || 0,
      angle: 0,
      length: length || 0,
      scale: 1,
      world: { x: 0, y: 0, a: 0 },
    };
    if (bone.parent) bone.parent.children.push(bone);
    return bone;
  };

  /** Resolves world transforms. Bones must be listed parent before child. */
  R.solve = function (bones) {
    for (const bone of bones) {
      const parent = bone.parent;
      if (!parent) {
        bone.world.x = bone.x;
        bone.world.y = bone.y;
        bone.world.a = bone.angle;
        continue;
      }
      const cos = Math.cos(parent.world.a);
      const sin = Math.sin(parent.world.a);
      bone.world.x = parent.world.x + bone.x * cos - bone.y * sin;
      bone.world.y = parent.world.y + bone.x * sin + bone.y * cos;
      bone.world.a = parent.world.a + bone.angle;
    }
  };

  /**
   * Two-bone IK in world space. Returns the world angle of the upper bone and
   * the lower bone's angle relative to it, so the caller can write
   * `upper.angle = a1 - parentWorldAngle; lower.angle = a2;`.
   *
   * `bend` picks the elbow side: 1 and -1 mirror the solution.
   */
  R.ik2 = function (rootX, rootY, upperLength, lowerLength, targetX, targetY, bend) {
    const dx = targetX - rootX;
    const dy = targetY - rootY;
    const raw = Math.hypot(dx, dy) || 1e-6;
    const reach = Math.min(
      upperLength + lowerLength - 1e-4,
      Math.max(Math.abs(upperLength - lowerLength) + 1e-4, raw),
    );
    const base = Math.atan2(dy, dx);
    const cos = (upperLength * upperLength + reach * reach - lowerLength * lowerLength) /
      (2 * upperLength * reach);
    const a1 = base + (bend || 1) * Math.acos(Math.max(-1, Math.min(1, cos)));
    const tipX = rootX + Math.cos(a1) * upperLength;
    const tipY = rootY + Math.sin(a1) * upperLength;
    const a2 = Math.atan2(targetY - tipY, targetX - tipX) - a1;
    return { a1, a2 };
  };

  /* ---------- parts ---------- */

  /**
   * A part is a draw function in a bone's frame. `ox`/`oy` place the origin
   * along the bone (usually the half-length), `angle` is a fixed rotation.
   */
  R.part = function (opts) {
    return Object.assign(
      { bone: null, ox: 0, oy: 0, angle: 0, scale: 1, alpha: 1, draw: function () {} },
      opts,
    );
  };

  R.drawPart = function (ctx, part, t) {
    const bone = part.bone;
    if (!bone) return;
    ctx.save();
    ctx.translate(bone.world.x, bone.world.y);
    ctx.rotate(bone.world.a);
    ctx.translate(part.ox || 0, part.oy || 0);
    ctx.rotate(part.angle || 0);
    ctx.scale(part.scale * bone.scale, part.scale * bone.scale);
    ctx.globalAlpha *= part.alpha;
    part.draw(ctx, part, bone, t || 0);
    ctx.restore();
  };

  /* ---------- springs ---------- */

  /**
   * A spring chain. `length` is the rest distance between neighbours; the
   * first point is pinned to the bone that owns it each step.
   */
  R.spring = function (opts) {
    return {
      count: Math.max(2, opts.count || 8),
      length: opts.length || 16,
      stiffness: opts.stiffness === undefined ? 0.55 : opts.stiffness,
      damping: opts.damping === undefined ? 0.22 : opts.damping,
      gravity: opts.gravity || 0,
      points: [],
      previous: [],
      ready: false,
      bone: null,
      ox: opts.ox || 0,
      oy: opts.oy || 0,
    };
  };

  R.resetSpring = function (spring, x, y) {
    spring.points.length = 0;
    spring.previous.length = 0;
    for (let i = 0; i < spring.count; i++) {
      spring.points.push({ x: x, y: y + i * spring.length });
      spring.previous.push({ x: x, y: y + i * spring.length });
    }
    spring.ready = true;
  };

  /** One Verlet step with distance constraints; `dt` is in seconds. */
  R.stepSpring = function (spring, rootX, rootY, dt) {
    if (!spring.ready) {
      R.resetSpring(spring, rootX, rootY);
      return;
    }
    const points = spring.points;
    const previous = spring.previous;
    const step = Math.min(dt, 0.05);
    const fall = spring.gravity * step * step;

    for (let i = 1; i < spring.count; i++) {
      const point = points[i];
      const vx = (point.x - previous[i].x) * (1 - spring.damping);
      const vy = (point.y - previous[i].y) * (1 - spring.damping);
      previous[i].x = point.x;
      previous[i].y = point.y;
      point.x += vx;
      point.y += vy + fall;
    }

    points[0].x = rootX;
    points[0].y = rootY;

    for (let pass = 0; pass < 3; pass++) {
      for (let i = 1; i < spring.count; i++) {
        const a = points[i - 1];
        const b = points[i];
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const distance = Math.hypot(dx, dy) || 1e-6;
        const correction = ((distance - spring.length) / distance) * spring.stiffness * 0.5;
        if (i === 1) {
          // The root is pinned, so the first link moves alone.
          b.x -= dx * correction * 2;
          b.y -= dy * correction * 2;
        } else {
          a.x += dx * correction;
          a.y += dy * correction;
          b.x -= dx * correction;
          b.y -= dy * correction;
        }
      }
    }
  };

  /** Steps a spring whose root sits at a bone's origin (plus an offset). */
  R.stepSpringOnBone = function (spring, bone, dt) {
    const cos = Math.cos(bone.world.a);
    const sin = Math.sin(bone.world.a);
    const x = bone.world.x + spring.ox * cos - spring.oy * sin;
    const y = bone.world.y + spring.ox * sin + spring.oy * cos;
    R.stepSpring(spring, x, y, dt);
  };

  /** A smooth polyline through the spring, for hair, rope or foam. */
  R.strokeSpring = function (ctx, spring, width, color) {
    const points = spring.points;
    if (points.length < 2) return;
    ctx.save();
    ctx.strokeStyle = color;
    ctx.lineWidth = width;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.beginPath();
    ctx.moveTo(points[0].x, points[0].y);
    for (let i = 1; i < points.length; i++) {
      const midX = (points[i - 1].x + points[i].x) / 2;
      const midY = (points[i - 1].y + points[i].y) / 2;
      ctx.quadraticCurveTo(points[i - 1].x, points[i - 1].y, midX, midY);
    }
    const last = points[points.length - 1];
    ctx.lineTo(last.x, last.y);
    ctx.stroke();
    ctx.restore();
  };

  /**
   * A damped scalar spring, for a value that has to follow another one with
   * lag (a head's turn carried into the hair, a shoulder into the coat).
   */
  R.springValue = function (state, target, stiffness, damping, dt) {
    const step = Math.min(dt, 0.05);
    const force = (target - state.value) * stiffness - state.velocity * damping;
    state.velocity += force * step;
    state.value += state.velocity * step;
    return state.value;
  };

  /* ---------- small maths ---------- */

  R.clamp = function (value, low, high) {
    return Math.min(high, Math.max(low, value));
  };

  R.mix = function (a, b, t) {
    return a + (b - a) * t;
  };

  R.smooth = function (t) {
    const p = R.clamp(t, 0, 1);
    return p * p * (3 - 2 * p);
  };

  R.wave = function (t, period, phase) {
    return Math.sin((t / period) * Math.PI * 2 + (phase || 0));
  };

  window.RIG = R;
})();
