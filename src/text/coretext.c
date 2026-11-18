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
    if (font && (CTFontGetSymbolicTraits(font) & kCTFontColorGlyphsTrait)) {
        CFRelease(font);
        return NULL;
    }
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
    uint32_t decoration;
    int32_t direction;
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
    CTWritingDirection direction=(CTWritingDirection)input[0].direction;
    CTParagraphStyleSetting setting={kCTParagraphStyleSpecifierBaseWritingDirection,sizeof(direction),&direction};
    CTParagraphStyleRef paragraph=CTParagraphStyleCreate(&setting,1);
    CFAttributedStringSetAttribute(attributed,CFRangeMake(0,CFAttributedStringGetLength(attributed)),kCTParagraphStyleAttributeName,paragraph);
    CFRelease(paragraph);
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
            double baseline=(double)rows/scale-metrics->baseline;
            CGContextSetTextPosition(ctx,0,baseline);CTLineDraw(line,ctx);
            CFIndex start=0;
            for(size_t i=0;i<count;i++) {
                const StyledRun *r=&input[i];
                CFStringRef string=CFStringCreateWithBytes(NULL,r->text,r->len,kCFStringEncodingUTF8,false);
                CFIndex end=start+CFStringGetLength(string);CFRelease(string);
                if(r->decoration) {
                    CTFontRef font=(CTFontRef)r->font;
                    double thickness=CTFontGetUnderlineThickness(font),position=CTFontGetXHeight(font)/2;
                    // OpenType OS/2 strikeout metrics are expressed in font units.
                    CFDataRef table=CTFontCopyTable(font,kCTFontTableOS2,kCTFontTableOptionNoOptions);
                    if(r->decoration==2 && table && CFDataGetLength(table)>=30) {
                        const uint8_t *p=CFDataGetBytePtr(table);
                        double units=CTFontGetSize(font)/CTFontGetUnitsPerEm(font);
                        thickness=(int16_t)((p[26]<<8)|p[27])*units;
                        position=(int16_t)((p[28]<<8)|p[29])*units;
                    }
                    if(table)CFRelease(table);
                    double x=CTLineGetOffsetForStringIndex(line,start,NULL);
                    double end_x=CTLineGetOffsetForStringIndex(line,end,NULL);
                    if(i+1==count)end_x-=r->tracking;
                    CGContextSetRGBFillColor(ctx,r->color[0],r->color[1],r->color[2],r->color[3]);
                    // Figma places the strike at the font's strikeout position,
                    // and the underline below the font's underline position.
                    double bottom=r->decoration==1
                        ? baseline+CTFontGetUnderlinePosition(font)-thickness*1.5
                        : baseline+position-thickness;
                    if(bottom<0 || bottom+thickness>(double)rows/scale || thickness<=0) {
                        status=6;break;
                    }
                    // Automatic decorations are continuous in the Figma export;
                    // custom placement/style/skip-ink controls are not supported.
                    CGContextFillRect(ctx,CGRectMake(x,bottom,end_x-x,thickness));
                }
                start=end;
            }
            CGContextRelease(ctx);
        }
    }
    CFRelease(line);CFRelease(attributed);CGColorSpaceRelease(space);return status;
}

// Natural height includes the font’s own leading, never a guessed multiplier.
double figleaf_ct_height(void *base) {
    CTFontRef font=(CTFontRef)base;
    return CTFontGetAscent(font)+CTFontGetDescent(font)+CTFontGetLeading(font);
}

// CoreFoundation uses UTF-16 ranges; the public layout API uses UTF-8 byte ends.
static uint32_t utf8_end(CFStringRef string, CFIndex end) {
    CFIndex bytes=0;
    CFStringGetBytes(string,CFRangeMake(0,end),kCFStringEncodingUTF8,0,false,NULL,0,&bytes);
    return (uint32_t)bytes;
}
int figleaf_ct_boundaries(const uint8_t *text,size_t len,uint32_t *graphemes,uint32_t *lines,uint32_t *counts) {
    CFStringRef string=CFStringCreateWithBytes(NULL,text,len,kCFStringEncodingUTF8,false);
    if(!string)return 2;
    CFIndex length=CFStringGetLength(string),at=0;counts[0]=counts[1]=0;
    while(at<length) {
        CFRange range=CFStringGetRangeOfComposedCharactersAtIndex(string,at);
        at=range.location+range.length;
        if(counts[0]>=512){CFRelease(string);return 4;}
        graphemes[counts[0]++]=utf8_end(string,at);
    }
    CFStringTokenizerRef tokenizer=CFStringTokenizerCreate(NULL,string,CFRangeMake(0,length),kCFStringTokenizerUnitLineBreak,NULL);
    if(!tokenizer){CFRelease(string);return 2;}
    while(CFStringTokenizerAdvanceToNextToken(tokenizer)!=kCFStringTokenizerTokenNone) {
        CFRange range=CFStringTokenizerGetCurrentTokenRange(tokenizer);
        uint32_t end=utf8_end(string,range.location+range.length);
        bool cluster=false;for(uint32_t i=0;i<counts[0];i++)if(graphemes[i]==end)cluster=true;
        if(cluster && (counts[1]==0 || lines[counts[1]-1]!=end)) {
            if(counts[1]>=512){CFRelease(tokenizer);CFRelease(string);return 4;}
            lines[counts[1]++]=end;
        }
    }
    if(counts[1]==0 || lines[counts[1]-1]!=len) {
        if(counts[1]>=512){CFRelease(tokenizer);CFRelease(string);return 4;}
        lines[counts[1]++]=(uint32_t)len;
    }
    CFRelease(tokenizer);CFRelease(string);return 0;
}
bool figleaf_ct_covers(void *base,const uint8_t *text,size_t len) {
    CTFontRef font=(CTFontRef)base;
    if(CTFontGetSymbolicTraits(font)&kCTFontColorGlyphsTrait)return false;
    CFStringRef string=CFStringCreateWithBytes(NULL,text,len,kCFStringEncodingUTF8,false);
    if(!string)return false;
    CFIndex count=CFStringGetLength(string);UniChar chars[512];CGGlyph glyphs[512];
    if(count>512){CFRelease(string);return false;}
    CFStringGetCharacters(string,CFRangeMake(0,count),chars);CFRelease(string);
    CTFontGetGlyphsForCharacters(font,chars,glyphs,count);
    for(CFIndex i=0;i<count;i++)if(!glyphs[i] && chars[i]!=0x200b && chars[i]!=0x200c && chars[i]!=0x200d && chars[i]!=0x2060)return false;
    return true;
}
int figleaf_ct_direction(const uint8_t *text,size_t len) {
    CFStringRef string=CFStringCreateWithBytes(NULL,text,len,kCFStringEncodingUTF8,false);
    if(!string)return 0;
    CFCharacterSetRef letters=CFCharacterSetGetPredefined(kCFCharacterSetLetter);
    CFCharacterSetRef marks=CFCharacterSetGetPredefined(kCFCharacterSetNonBase);
    int direction=0;
    // First strong letter for the supported BMP scripts. Explicit bidi controls
    // and script blocks outside this bounded profile are rejected by Rust.
    for(CFIndex i=0;i<CFStringGetLength(string);i++) {
        UniChar c=CFStringGetCharacterAtIndex(string,i);
        if(CFCharacterSetIsCharacterMember(letters,c) && !CFCharacterSetIsCharacterMember(marks,c)) {
            direction=(c>=0x0590 && c<=0x08ff) || (c>=0xfb1d && c<=0xfdff) || (c>=0xfe70 && c<=0xfefc);break;
        }
    }
    CFRelease(string);return direction;
}
