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
    vec4 positionRadiusSq;  // xyz = position, w = radius²
    vec4 colorIntensity;    // rgb = color * intensity (pre-multiplied), w = invRadius
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

    // Normal is already normalized from vertex shader
    vec3 N = fragNormal;

    // Ambient base
    vec3 totalLight = lighting.ambient.rgb;

    // Accumulate point light contributions
    uint count = min(lighting.lightCount, 128u);
    for (uint i = 0u; i < count; i++) {
        vec3  lightPos   = lighting.lights[i].positionRadiusSq.xyz;
        float radiusSq   = lighting.lights[i].positionRadiusSq.w;
        vec3  lightColI  = lighting.lights[i].colorIntensity.rgb; // pre-multiplied color*intensity
        float invRadius  = lighting.lights[i].colorIntensity.w;

        vec3  toLight = lightPos - fragWorldPos;
        float distSq  = dot(toLight, toLight);

        // Early out using squared distance (no sqrt)
        if (distSq >= radiusSq) continue;

        // inversesqrt is a single HW instruction on most GPUs
        float invDist = inversesqrt(distSq);

        vec3  L = toLight * invDist;

        // Smooth quadratic attenuation: t = 1 - dist/radius = 1 - dist*invRadius
        float dist = distSq * invDist;  // dist = distSq / sqrt(distSq)
        float t = 1.0 - dist * invRadius;
        float atten = t * t;

        // Diffuse: two-sided so geometry with flipped normals still receives light
        float NdotL = abs(dot(N, L));

        totalLight += lightColI * NdotL * atten;
    }

    outColor = vec4(texColor.rgb * totalLight, 1.0);
}
