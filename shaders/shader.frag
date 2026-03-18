#version 450

// Input from vertex shader
layout(location = 0) in vec3 fragColor;
layout(location = 1) in vec2 fragTexCoord;
layout(location = 2) in vec3 fragWorldPos;
layout(location = 3) in vec3 fragNormal;

// Uniforms
layout(binding = 1) uniform sampler2D texSampler;

// Point light structure (matches Rust GpuLight)
struct PointLight {
    vec4 positionRadius;  // xyz = position, w = radius
    vec4 colorIntensity;  // rgb = color,    w = intensity
};

// Lighting UBO
layout(binding = 2) uniform LightingData {
    vec4 cameraPos;       // xyz = camera world position
    vec4 ambient;         // rgb = ambient light color
    uint lightCount;
    uint _pad0;
    uint _pad1;
    uint _pad2;
    PointLight lights[128];
} lighting;

// Push constants for opacity
layout(push_constant) uniform PushConstants {
    layout(offset = 64) float opacity;
} push;

// Output
layout(location = 0) out vec4 outColor;

void main() {
    vec4 texColor = texture(texSampler, fragTexCoord);

    // Guard against zero-length normals (would produce NaN and make everything black)
    float nLen = length(fragNormal);
    vec3 N = nLen > 0.001 ? fragNormal / nLen : vec3(0.0, 0.0, 1.0);

    // Ambient base
    vec3 totalLight = lighting.ambient.rgb;

    // Accumulate point light contributions
    uint count = min(lighting.lightCount, 128u);
    for (uint i = 0u; i < count; i++) {
        vec3  lightPos   = lighting.lights[i].positionRadius.xyz;
        float lightRad   = lighting.lights[i].positionRadius.w;
        vec3  lightCol   = lighting.lights[i].colorIntensity.rgb;
        float lightInt   = lighting.lights[i].colorIntensity.w;

        vec3  toLight = lightPos - fragWorldPos;
        float dist    = length(toLight);

        if (dist >= lightRad) continue;

        vec3  L = toLight / dist;

        // Smooth quadratic attenuation (reaches zero at radius)
        float t = 1.0 - dist / lightRad;
        float atten = t * t;

        // Diffuse: two-sided so geometry with flipped normals still receives light
        float NdotL = abs(dot(N, L));

        totalLight += lightCol * lightInt * NdotL * atten;
    }

    outColor = vec4(texColor.rgb * totalLight, 1.0);
}
