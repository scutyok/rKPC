#version 450

// Vertex attributes
layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;
layout(location = 2) in vec2 inTexCoord;
layout(location = 3) in vec3 inNormal;

// Uniforms
layout(binding = 0) uniform UniformBufferObject {
    mat4 view;
    mat4 proj;
} ubo;

// Push constants for model matrix
layout(push_constant) uniform PushConstants {
    mat4 model;
} push;

// Output to fragment shader
layout(location = 0) out vec3 fragColor;
layout(location = 1) out vec2 fragTexCoord;
layout(location = 2) out vec3 fragWorldPos;
layout(location = 3) out vec3 fragNormal;

void main() {
    vec4 worldPos = push.model * vec4(inPosition, 1.0);
    gl_Position = ubo.proj * ubo.view * worldPos;
    fragColor = inColor;
    fragTexCoord = inTexCoord;
    fragWorldPos = worldPos.xyz;
    // Transform normal by model matrix and normalize in vertex shader
    // Guard against zero-length normals (would produce NaN)
    vec3 n = mat3(push.model) * inNormal;
    float nLen = length(n);
    fragNormal = nLen > 0.001 ? n / nLen : vec3(0.0, 0.0, 1.0);
}
