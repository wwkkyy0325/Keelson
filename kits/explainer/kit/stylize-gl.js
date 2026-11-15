// The restyle pass, on the GPU.
//
// The core is a Kuwahara filter: for every pixel it measures four
// neighbourhoods, keeps the one with the flattest colour, and paints with
// that average. Flat areas become brush blocks, edges stay where they are.
// That is what makes a frame look painted; posterizing alone just looks
// printed. A light edge multiply keeps the drawing readable.
//
// One WebGL context is reused for every frame and image, sized to the
// request. `STYLIZE_GL.oil` returns a canvas like `STYLIZE.oil`, so a scene
// can prefer it and fall back without knowing which ran.
(function () {
  const SGL = {};
  let gl = null;
  let canvas = null;
  let program = null;
  let buffer = null;
  let texture = null;
  let size = { width: 0, height: 0 };

  const VERTEX = `#version 300 es
    in vec2 position;
    out vec2 vUv;
    void main() {
      vUv = position * 0.5 + 0.5;
      gl_Position = vec4(position, 0.0, 1.0);
    }`;

  const FRAGMENT = `#version 300 es
    precision highp float;
    uniform sampler2D uImage;
    uniform vec2 uSize;
    uniform float uRadius;
    uniform float uLevels;
    uniform float uInk;
    in vec2 vUv;
    out vec4 outColor;

    float lum(vec3 color) { return dot(color, vec3(0.299, 0.587, 0.114)); }

    void main() {
      vec2 px = 1.0 / uSize;
      int radius = int(uRadius);

      vec3 bestColor = vec3(0.0);
      float bestVariance = 1.0e9;

      for (int quad = 0; quad < 4; quad++) {
        vec2 dir = vec2(quad == 0 || quad == 2 ? -1.0 : 1.0,
                        quad < 2 ? -1.0 : 1.0);
        vec3 sum = vec3(0.0);
        vec3 sumSq = vec3(0.0);
        float count = 0.0;
        for (int i = -3; i <= 3; i++) {
          if (abs(i) > radius) continue;
          for (int j = -3; j <= 3; j++) {
            if (abs(j) > radius) continue;
            vec2 at = vUv + dir * vec2(float(i), float(j)) * px;
            vec3 colour = texture(uImage, at).rgb;
            sum += colour;
            sumSq += colour * colour;
            count += 1.0;
          }
        }
        vec3 mean = sum / count;
        vec3 variance = sumSq / count - mean * mean;
        float score = lum(variance);
        if (score < bestVariance) {
          bestVariance = score;
          bestColor = mean;
        }
      }

      float levels = max(2.0, uLevels);
      vec3 painted = floor(bestColor * levels + 0.5) / levels;

      float l00 = lum(texture(uImage, vUv + vec2(-px.x, -px.y)).rgb);
      float l02 = lum(texture(uImage, vUv + vec2( px.x, -px.y)).rgb);
      float l10 = lum(texture(uImage, vUv + vec2(-px.x,  0.0)).rgb);
      float l12 = lum(texture(uImage, vUv + vec2( px.x,  0.0)).rgb);
      float l20 = lum(texture(uImage, vUv + vec2(-px.x,  px.y)).rgb);
      float l22 = lum(texture(uImage, vUv + vec2( px.x,  px.y)).rgb);
      float gx = -l00 - 2.0 * l10 - l20 + l02 + 2.0 * l12 + l22;
      float gy = -l00 - 2.0 * l02 - l22 + l20 + 2.0 * l12 + l22;
      float edge = clamp(length(vec2(gx, gy)), 0.0, 1.0);

      float grit = (fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453) - 0.5) * 0.035;
      vec3 colour = clamp(painted * (1.0 - edge * uInk) + grit, 0.0, 1.0);
      float alpha = texture(uImage, vUv).a;
      outColor = vec4(colour, alpha);
    }`;

  function compile(type, source) {
    const shader = gl.createShader(type);
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      throw new Error(gl.getShaderInfoLog(shader) || "shader failed");
    }
    return shader;
  }

  function setup(width, height) {
    if (!canvas) {
      canvas = document.createElement("canvas");
      gl = canvas.getContext("webgl2", {
        antialias: false,
        alpha: true,
        premultipliedAlpha: false,
        preserveDrawingBuffer: true,
        powerPreference: "low-power",
      });
      if (!gl) return null;
      program = gl.createProgram();
      gl.attachShader(program, compile(gl.VERTEX_SHADER, VERTEX));
      gl.attachShader(program, compile(gl.FRAGMENT_SHADER, FRAGMENT));
      gl.linkProgram(program);
      if (!gl.getProgramParameter(program, gl.LINK_STATUS)) return null;
      buffer = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
      texture = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    }
    if (size.width !== width || size.height !== height) {
      canvas.width = width;
      canvas.height = height;
      size = { width, height };
    }
    return gl;
  }

  SGL.available = function () {
    try {
      return !!document.createElement("canvas").getContext("webgl2");
    } catch (error) {
      return false;
    }
  };

  function render(image, opts) {
    const scale = opts.scale === undefined ? 1 : opts.scale;
    const width = Math.max(1, Math.round(image.width * scale));
    const height = Math.max(1, Math.round(image.height * scale));
    if (!setup(width, height)) return null;

    gl.viewport(0, 0, width, height);
    gl.useProgram(program);

    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, image);

    const position = gl.getAttribLocation(program, "position");
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.enableVertexAttribArray(position);
    gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0);

    gl.uniform1i(gl.getUniformLocation(program, "uImage"), 0);
    gl.uniform2f(gl.getUniformLocation(program, "uSize"), width, height);
    gl.uniform1f(gl.getUniformLocation(program, "uRadius"), opts.radius === undefined ? 2 : opts.radius);
    gl.uniform1f(gl.getUniformLocation(program, "uLevels"), opts.levels === undefined ? 24 : opts.levels);
    gl.uniform1f(gl.getUniformLocation(program, "uInk"), opts.ink === undefined ? 0.35 : opts.ink);

    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.finish();

    // readPixels, not drawImage: copying the GL canvas through the 2D
    // compositor shifted the hue on a software driver. The pixels themselves
    // are exact, and they arrive bottom-up, so the rows are flipped here.
    const pixels = new Uint8Array(width * height * 4);
    gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);

    const out = document.createElement("canvas");
    out.width = width;
    out.height = height;
    const octx = out.getContext("2d");
    const image = octx.createImageData(width, height);
    const rowBytes = width * 4;
    for (let y = 0; y < height; y++) {
      const from = (height - 1 - y) * rowBytes;
      image.data.set(pixels.subarray(from, from + rowBytes), y * rowBytes);
    }
    octx.putImageData(image, 0, 0);
    return out;
  }

  SGL.oil = function (image, opts) {
    const options = opts || {};
    if (SGL.available()) {
      try {
        const painted = render(image, options);
        if (painted) return painted;
      } catch (error) {
        // A driver that refuses the shader is a reason to use the CPU.
      }
    }
    return window.STYLIZE.oil(image, options);
  };

  window.STYLIZE_GL = SGL;
})();
