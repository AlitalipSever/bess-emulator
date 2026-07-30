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
out vec3 vNormal;
out vec3 vColor;
out vec3 vWorld;
out float vEmissive;
void main() {
  vec3 world = aPos * iScale + iOffset;
  if (uShadow > 0.5) {
    float h = max(world.y - uFloorY, 0.0);
    // Flatten 5 cm above the floor: clearly above every ground surface
    // (pads at 2 cm, lane markings at 3 cm), or the coplanar shadow quads
    // z-fight with them and shimmer while the camera moves.
    world = vec3(world.x + h * 0.42, uFloorY + 0.05, world.z + h * 0.22);
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
uniform vec3 uLightDir;   // normalized, pointing FROM the light
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
