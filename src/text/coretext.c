// Synchronous, retained-font CoreText bridge. Rust validates byte/text/bitmap
// budgets before calling. Every Create/Copy reference is released here or owned
// by the Rust Font/Instance guard. No font registration or system fallback API.
#include <CoreText/CoreText.h>
#include <CoreGraphics/CoreGraphics.h>
#include <stdint.h>
#include <string.h>

typedef struct {
    double advance, ascent, descent, baseline;
    uint32_t glyphs, lines;
} Metrics;

void *figleaf_ct_font(const uint8_t *bytes, size_t len) {
    CFDataRef data = CFDataCreate(NULL, bytes, len);
    if (!data) return NULL;
    CGDataProviderRef provider = CGDataProviderCreateWithCFData(data);
    CFRelease(data);
    if (!provider) return NULL;
    CGFontRef font = CGFontCreateWithDataProvider(provider);
    CGDataProviderRelease(provider);
    return (void *)font;
}

void figleaf_ct_drop(void *font) {
    CGFontRelease((CGFontRef)font);
}

// Applies exactly the caller's axis values. An axis left out (optical size in
// particular) is resolved by CoreText at the logical point size.
void *figleaf_ct_instance(void *base, double size, const uint32_t *tags,
                        const double *axis_values, size_t count) {
    CFMutableDictionaryRef axes = CFDictionaryCreateMutable(
        NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    for (size_t i = 0; i < count; i++) {
        int tag = (int)tags[i];
        CFNumberRef key = CFNumberCreate(NULL, kCFNumberIntType, &tag);
        CFNumberRef value = CFNumberCreate(NULL, kCFNumberDoubleType, &axis_values[i]);
        CFDictionarySetValue(axes, key, value);
        CFRelease(key);
        CFRelease(value);
    }
    const void *keys[] = {kCTFontVariationAttribute};
    const void *values[] = {axes};
    CFDictionaryRef attrs = CFDictionaryCreate(
        NULL, keys, values, 1,
        &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    CFRelease(axes);
    CTFontDescriptorRef descriptor = CTFontDescriptorCreateWithAttributes(attrs);
    CFRelease(attrs);
    CTFontRef font = CTFontCreateWithGraphicsFont((CGFontRef)base, size, NULL, descriptor);
    CFRelease(descriptor);
    return (void *)font;
}

void figleaf_ct_instance_drop(void *font) {
    CFRelease(font);
}

int figleaf_ct_run(void *base, const uint8_t *text, size_t len,
                 double height, double scale, uint8_t *pixels,
                 uint32_t width, uint32_t rows, Metrics *metrics) {
    CTFontRef font = (CTFontRef)base;
    CFStringRef string = CFStringCreateWithBytes(
        NULL, text, len, kCFStringEncodingUTF8, false);
    if (!string) return 2;
    const void *keys[] = {kCTFontAttributeName, kCTForegroundColorFromContextAttributeName};
    const void *values[] = {font, kCFBooleanTrue};
    CFDictionaryRef attrs = CFDictionaryCreate(
        NULL, keys, values, 2,
        &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    CFAttributedStringRef attributed = CFAttributedStringCreate(NULL, string, attrs);
    CFRelease(attrs);
    CFRelease(string);
    CTLineRef line = CTLineCreateWithAttributedString(attributed);
    CFRelease(attributed);
    double ascent = CTFontGetAscent(font), descent = CTFontGetDescent(font);
    metrics->advance = CTLineGetTypographicBounds(line, NULL, NULL, NULL);
    metrics->ascent = ascent;
    metrics->descent = descent;
    metrics->baseline = (height - ascent - descent) / 2 + ascent;
    metrics->glyphs = (uint32_t)CTLineGetGlyphCount(line);
    metrics->lines = 1;

    int status = 0;
    CFArrayRef runs = CTLineGetGlyphRuns(line);
    for (CFIndex i = 0; i < CFArrayGetCount(runs); i++) {
        CTRunRef run = (CTRunRef)CFArrayGetValueAtIndex(runs, i);
        CTFontRef actual = (CTFontRef)CFDictionaryGetValue(
            CTRunGetAttributes(run), kCTFontAttributeName);
        CFStringRef actual_family = CTFontCopyFamilyName(actual);
        CFStringRef expected_family = CTFontCopyFamilyName(font);
        if (!CFEqual(actual_family, expected_family)) status = 3;
        CFRelease(actual_family);
        CFRelease(expected_family);
        CFIndex count = CTRunGetGlyphCount(run);
        CGGlyph glyphs[512];
        if (count > 512) { status = 4; break; }
        CTRunGetGlyphs(run, CFRangeMake(0, count), glyphs);
        for (CFIndex j = 0; j < count; j++) {
            if (glyphs[j] == 0) status = 3;
        }
    }
    if (!status && pixels) {
        memset(pixels, 0, (size_t)width * rows);
        CGContextRef ctx = CGBitmapContextCreate(
            pixels, width, rows, 8, width, NULL, kCGImageAlphaOnly);
        if (!ctx) {
            status = 5;
        } else {
            CGContextSetAllowsFontSmoothing(ctx, false);
            CGContextSetShouldSmoothFonts(ctx, false);
            CGContextSetShouldAntialias(ctx, true);
            CGContextSetShouldSubpixelPositionFonts(ctx, true);
            CGContextSetShouldSubpixelQuantizeFonts(ctx, false);
            CGContextScaleCTM(ctx, scale, scale);
            CGContextSetGrayFillColor(ctx, 1, 1);
            CGContextSetTextPosition(ctx, 0, height - metrics->baseline);
            CTLineDraw(line, ctx);
            CGContextRelease(ctx);
        }
    }
    CFRelease(line);
    return status;
}
