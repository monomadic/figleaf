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
                 double height, double tracking, double scale, uint8_t *pixels,
                 uint32_t width, uint32_t rows, Metrics *metrics) {
    CTFontRef font = (CTFontRef)base;
    CFStringRef string = CFStringCreateWithBytes(
        NULL, text, len, kCFStringEncodingUTF8, false);
    if (!string) return 2;
    // Tracking is added after every glyph, as Figma's letter spacing is.
    CFNumberRef kern = CFNumberCreate(NULL, kCFNumberDoubleType, &tracking);
    const void *keys[] = {kCTFontAttributeName, kCTForegroundColorFromContextAttributeName,
                          kCTKernAttributeName};
    const void *values[] = {font, kCFBooleanTrue, kern};
    CFDictionaryRef attrs = CFDictionaryCreate(
        NULL, keys, values, 3,
        &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    CFRelease(kern);
    CFAttributedStringRef attributed = CFAttributedStringCreate(NULL, string, attrs);
    CFRelease(attrs);
    CFRelease(string);
    CTLineRef line = CTLineCreateWithAttributedString(attributed);
    CFRelease(attributed);
    double ascent = CTFontGetAscent(font), descent = CTFontGetDescent(font);
    // The space after the last glyph is not part of the line's width.
    metrics->advance = CTLineGetTypographicBounds(line, NULL, NULL, NULL);
    if (CTLineGetGlyphCount(line) > 0) metrics->advance -= tracking;
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
            CGContextSetTextPosition(ctx, 0, (double)rows / scale - metrics->baseline);
            CTLineDraw(line, ctx);
            CGContextRelease(ctx);
        }
    }
    CFRelease(line);
    return status;
}

// Each run carries a UTF-8 string. CoreText ranges are derived here in UTF-16.
// One attributed CTLine preserves shaping across color and font boundaries.
typedef struct {
    void *font;
    const uint8_t *text;
    size_t len;
    double height, tracking;
    double color[4];
} StyledRun;
int figleaf_ct_runs(const StyledRun *input, size_t count, double scale,
                   uint8_t *pixels, uint32_t width, uint32_t rows, Metrics *metrics) {
    CFMutableAttributedStringRef attributed=CFAttributedStringCreateMutable(NULL,0);
    CGColorSpaceRef space=CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    double ascent=0,descent=0,height=0;
    int status=0;
    for(size_t i=0;i<count;i++) {
        const StyledRun *r=&input[i];CTFontRef font=(CTFontRef)r->font;
        CFStringRef string=CFStringCreateWithBytes(NULL,r->text,r->len,kCFStringEncodingUTF8,false);
        if(!string){status=2;break;}
        CFIndex start=CFAttributedStringGetLength(attributed),length=CFStringGetLength(string);
        CFAttributedStringReplaceString(attributed,CFRangeMake(start,0),string);CFRelease(string);
        CFRange range=CFRangeMake(start,length);
        CFNumberRef kern=CFNumberCreate(NULL,kCFNumberDoubleType,&r->tracking);
        CGColorRef color=CGColorCreate(space,r->color);
        CFAttributedStringSetAttribute(attributed,range,kCTFontAttributeName,font);
        CFAttributedStringSetAttribute(attributed,range,kCTKernAttributeName,kern);
        CFAttributedStringSetAttribute(attributed,range,kCTForegroundColorAttributeName,color);
        CFRelease(kern);CGColorRelease(color);
        double a=CTFontGetAscent(font),d=CTFontGetDescent(font);
        if(a>ascent)ascent=a;if(d>descent)descent=d;if(r->height>height)height=r->height;
    }
    CTLineRef line=CTLineCreateWithAttributedString(attributed);
    metrics->advance=CTLineGetTypographicBounds(line,NULL,NULL,NULL);
    if(CTLineGetGlyphCount(line)>0)metrics->advance-=input[count-1].tracking;
    metrics->ascent=ascent;metrics->descent=descent;
    metrics->baseline=(height-ascent-descent)/2+ascent;
    metrics->glyphs=(uint32_t)CTLineGetGlyphCount(line);metrics->lines=1;
    CFArrayRef runs=CTLineGetGlyphRuns(line);
    for(CFIndex i=0;i<CFArrayGetCount(runs);i++) {
        CTRunRef run=(CTRunRef)CFArrayGetValueAtIndex(runs,i);
        CFIndex index=CTRunGetStringRange(run).location;
        CTFontRef expected=(CTFontRef)CFAttributedStringGetAttribute(attributed,index,kCTFontAttributeName,NULL);
        CTFontRef actual=(CTFontRef)CFDictionaryGetValue(CTRunGetAttributes(run),kCTFontAttributeName);
        CFStringRef a=CTFontCopyPostScriptName(actual),e=CTFontCopyPostScriptName(expected);
        if(!CFEqual(a,e))status=3;CFRelease(a);CFRelease(e);
        CFIndex n=CTRunGetGlyphCount(run);CGGlyph glyphs[512];
        if(n>512){status=4;break;}CTRunGetGlyphs(run,CFRangeMake(0,n),glyphs);
        for(CFIndex j=0;j<n;j++)if(!glyphs[j])status=3;
    }
    if(!status && pixels) {
        memset(pixels,0,(size_t)width*rows*4);
        CGContextRef ctx=CGBitmapContextCreate(pixels,width,rows,8,width*4,space,kCGImageAlphaPremultipliedLast|kCGBitmapByteOrder32Big);
        if(!ctx)status=5;else {
            CGContextSetAllowsFontSmoothing(ctx,false);CGContextSetShouldSmoothFonts(ctx,false);
            CGContextSetShouldAntialias(ctx,true);CGContextSetShouldSubpixelPositionFonts(ctx,true);
            CGContextSetShouldSubpixelQuantizeFonts(ctx,false);CGContextScaleCTM(ctx,scale,scale);
            CGContextSetTextPosition(ctx,0,(double)rows/scale-metrics->baseline);CTLineDraw(line,ctx);CGContextRelease(ctx);
        }
    }
    CFRelease(line);CFRelease(attributed);CGColorSpaceRelease(space);return status;
}

// Natural height includes the font’s own leading, never a guessed multiplier.
double figleaf_ct_height(void *base) {
    CTFontRef font=(CTFontRef)base;
    return CTFontGetAscent(font)+CTFontGetDescent(font)+CTFontGetLeading(font);
}
