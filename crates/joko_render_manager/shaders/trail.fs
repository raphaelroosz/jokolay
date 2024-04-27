#version 450

layout(location = 0) in vec2 vtex_coord;
layout(location = 1) in float valpha;
layout(location = 2) in vec4 vcolor;

layout(location = 0) out vec4 ocolor;

layout(location = 1) uniform sampler2D sam; // wrap_s = "REPEAT" wrap_t = "REPEAT"
layout(location = 3) uniform vec2 scroll_texture;

void main() {
    //vec4 color = texture(sam, vec2 (vtex_coord.x  + scroll_texture.x, vtex_coord.y + scroll_texture.y), -2.0);
    vec4 color = texture(sam, vec2 (vtex_coord.x  + 0.0, vtex_coord.y + scroll_texture.y), -2.0);
    //vec4 color = texture(sam, vtex_coord + scroll_texture);
    //vec4 color = texture(sam, vtex_coord, -2.0);//original working
    color.a = color.a * valpha;
    if (color.a < 0.01) {
        discard;
    }
    ocolor = color;
}
