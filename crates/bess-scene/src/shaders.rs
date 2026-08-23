//! GLSL sources, composed per platform: WebGL2 wants `#version 300 es` plus
//! explicit precision, desktop OpenGL wants `#version 330 core` and no
//! precision qualifiers. Both stages declare shared uniforms with matching
//! precision; a mediump/highp mismatch is a link error on ANGLE.

/// Vertex shader body (headerless).
const VS_BODY: &str = r"
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNormal;
layout(location=2) in vec3 iOffset;
layout(location=3) in vec3 iScale;
layout(location=4) in vec3 iColor;
layout(location=5) in float iEmissive;
uniform mat4 uViewProj;
uniform float uShadow;   // 1.0 = flatten onto the pad as a fake shadow
uniform float uFloorY;
uniform vec3 uLightDir;  // normalized, pointing FROM the light
out vec3 vNormal;
out vec3 vColor;
out vec3 vWorld;
out float vEmissive;
void main() {
  vec3 world = aPos * iScale + iOffset;
  if (uShadow > 0.5) {
    float h = max(world.y - uFloorY, 0.0);
    // Project along the light instead of along a constant. A point at
    // height h casts where the ray leaving it hits the floor, so the shadow
    // is short under a high sun, long under a low one, and it swings east
    // to west through the day. This used to be a hardcoded offset, which
    // meant the shadows never moved however the sun did.
    //
    // The drop is clamped because the true length goes to infinity as the
    // sun reaches the horizon: at the clamp a 3 m container throws about
    // 25 m, which reads as a long evening shadow and still lands on the
    // site rather than in the fog.
    float drop = max(-uLightDir.y, 0.12);
    float reach = h / drop;
    // Flatten 5 cm above the floor: clearly above every ground surface
    // (pads at 2 cm, lane markings at 3 cm), or the coplanar shadow quads
    // z-fight with them and shimmer while the camera moves.
    world = vec3(
      world.x + uLightDir.x * reach,
      uFloorY + 0.05,
      world.z + uLightDir.z * reach
    );
  }
  gl_Position = uViewProj * vec4(world, 1.0);
  vNormal = aNormal;
  vColor = iColor;
  vWorld = world;
  vEmissive = iEmissive;
}";

/// Fragment shader body (headerless).
const FS_BODY: &str = r"
in vec3 vNormal;
in vec3 vColor;
in vec3 vWorld;
in float vEmissive;
uniform vec3 uLightDir;   // shared with the vertex stage; see the note there
uniform vec3 uLightColor; // sun/moon color, premultiplied by intensity
uniform vec3 uSky;
uniform vec3 uGround;
uniform vec3 uFog;
uniform vec3 uEye;
uniform float uShadow;
uniform vec2 uFogRange;   // fog start / full distance, metres
out vec4 outColor;
void main() {
  if (uShadow > 0.5) { outColor = vec4(0.05, 0.05, 0.06, 0.22); return; }
  vec3 n = normalize(vNormal);
  float diff = max(dot(n, -uLightDir), 0.0);
  vec3 hemi = mix(uGround, uSky, n.y * 0.5 + 0.5);
  vec3 view = normalize(uEye - vWorld);
  vec3 half_v = normalize(view - uLightDir);
  float spec = pow(max(dot(n, half_v), 0.0), 40.0) * 0.22;
  vec3 lit = vColor * (hemi + uLightColor * diff) + uLightColor * spec;
  lit += vColor * vEmissive * 1.7;
  float f = smoothstep(uFogRange.x, uFogRange.y, length(vWorld - uEye));
  outColor = vec4(mix(lit, uFog, f), 1.0);
}";

/// Present-pass vertex shader: a fullscreen triangle from `gl_VertexID`,
/// no vertex buffers involved.
const PRESENT_VS_BODY: &str = r"
out vec2 vUv;
void main() {
  vec2 uv = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  vUv = uv;
  gl_Position = vec4(uv * 2.0 - 1.0, 0.0, 1.0);
}";

/// Present-pass fragment shader: sample the offscreen scene texture.
const PRESENT_FS_BODY: &str = r"
in vec2 vUv;
uniform sampler2D uScene;
out vec4 outColor;
void main() {
  outColor = texture(uScene, vUv);
}";

/// Sky fragment shader: everything above the horizon that is not an object.
///
/// Drawn as a fullscreen pass before the scene, so the sky is a picture
/// rather than a clear colour. Three things live in it and they answer to
/// different masters. The gradient and the cloud shapes are art direction.
/// Where the sun sits is not: it comes from the same computed solar position
/// that lights the site, so the disc is over the horizon it should be over
/// and moves through the day the way it should. How big it is drawn is art
/// direction again, a little larger than the half degree it subtends, or it
/// would be two pixels and nobody would find it.
///
/// Cloud cover is the measured okta series. Coverage is a threshold on
/// fractal noise, so 0 okta leaves the sky empty and 8 okta closes it over,
/// and the deck drifts with the observed wind rather than on a timer.
const SKY_FS_BODY: &str = r"
in vec2 vUv;
uniform vec3 uRayF;
uniform vec3 uRayR;
uniform vec3 uRayU;
uniform vec3 uLightDir;    // normalized, pointing FROM the light
uniform vec3 uZenith;
uniform vec3 uHorizon;
uniform vec3 uSunColor;
uniform float uCloud;      // 0 clear .. 1 overcast
uniform float uDaylight;   // 0 night .. 1 sun overhead
uniform float uTime;       // animation seconds
uniform vec2 uWind;        // drift, m/s, in world x/z
out vec4 outColor;

float hash21(vec2 p) {
  return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

float vnoise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  float a = hash21(i);
  float b = hash21(i + vec2(1.0, 0.0));
  float c = hash21(i + vec2(0.0, 1.0));
  float d = hash21(i + vec2(1.0, 1.0));
  return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float fbm(vec2 p) {
  float v = 0.0;
  float amp = 0.5;
  for (int i = 0; i < 5; i++) {
    v += amp * vnoise(p);
    p *= 2.03;
    amp *= 0.5;
  }
  return v;
}

void main() {
  vec3 dir = normalize(uRayF + (vUv.x * 2.0 - 1.0) * uRayR + (vUv.y * 2.0 - 1.0) * uRayU);

  // Gradient: deepest overhead, palest at the horizon.
  float up = clamp(dir.y, 0.0, 1.0);
  vec3 col = mix(uHorizon, uZenith, pow(up, 0.55));

  // The sun, where the solar position says it is.
  float sd = dot(dir, -uLightDir);
  float disc = smoothstep(cos(radians(1.5)), cos(radians(1.0)), sd);
  float glow = pow(max(sd, 0.0), 220.0) * 0.55 + pow(max(sd, 0.0), 14.0) * 0.12;
  float above = smoothstep(-0.02, 0.06, -uLightDir.y);
  col += uSunColor * (disc * 1.5 + glow) * above;

  // Cloud deck, on a plane the ray is projected onto. Only above the
  // horizon, and faded out near it so the deck does not end in a hard line.
  if (dir.y > 0.015 && uCloud > 0.001) {
    vec2 plane = dir.xz * (1200.0 / dir.y) * 0.0011 + uWind * uTime * 0.004;
    float n = fbm(plane);
    // Threshold slides with coverage: at 0 okta nothing clears it, at 8 the
    // sky is closed over.
    float edge = 1.02 - uCloud * 1.02;
    float cover = smoothstep(edge, edge + 0.30, n);
    cover *= smoothstep(0.015, 0.16, dir.y);
    // Lit from the sun side, grey underneath, and never brighter than day.
    float lit = 0.45 + 0.55 * clamp(uDaylight, 0.0, 1.0);
    vec3 cloud = mix(vec3(0.30, 0.32, 0.36), vec3(0.97, 0.97, 0.98), lit);
    cloud += uSunColor * pow(max(sd, 0.0), 6.0) * 0.25 * above;
    col = mix(col, cloud, cover * 0.92);
  }

  outColor = vec4(col, 1.0);
}";

/// Sky fragment shader source for the target platform.
pub fn sky_fragment(es: bool) -> String {
    compose(SKY_FS_BODY, es)
}

/// Present vertex shader source for the target platform.
pub fn present_vertex(es: bool) -> String {
    compose(PRESENT_VS_BODY, es)
}

/// Present fragment shader source for the target platform.
pub fn present_fragment(es: bool) -> String {
    compose(PRESENT_FS_BODY, es)
}

/// Compose a stage source for the target platform.
fn compose(body: &str, es: bool) -> String {
    if es {
        format!("#version 300 es\nprecision highp float;\n{body}")
    } else {
        format!("#version 330 core\n{body}")
    }
}

/// Vertex shader source for the target platform.
pub fn vertex(es: bool) -> String {
    compose(VS_BODY, es)
}

/// Fragment shader source for the target platform.
pub fn fragment(es: bool) -> String {
    compose(FS_BODY, es)
}

#[cfg(test)]
mod tests {
    #[test]
    fn es_sources_carry_version_and_precision() {
        let vs = super::vertex(true);
        assert!(vs.starts_with("#version 300 es"));
        assert!(vs.contains("precision highp float;"));
        let fs = super::fragment(true);
        assert!(fs.starts_with("#version 300 es"));
    }

    #[test]
    fn desktop_sources_have_no_precision_qualifier() {
        let fs = super::fragment(false);
        assert!(fs.starts_with("#version 330 core"));
        assert!(!fs.contains("precision"));
    }
}
