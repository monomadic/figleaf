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

// Verify a requested feature exists in the supplied font, instead of accepting
// CoreText's silent no-op for unknown tags. All table reads are bounds checked.
static uint16_t be16(const uint8_t *p) { return (uint16_t)((p[0]<<8)|p[1]); }
static uint32_t be32(const uint8_t *p) { return ((uint32_t)be16(p)<<16)|be16(p+2); }
static bool has_feature(CTFontRef font, uint32_t tag) {
    for(int i=0;i<2;i++) {
        CFDataRef data=CTFontCopyTable(font,i?kCTFontTableGPOS:kCTFontTableGSUB,kCTFontTableOptionNoOptions);
        if(!data)continue;
        const uint8_t *p=CFDataGetBytePtr(data);size_t len=(size_t)CFDataGetLength(data);
        bool found=false;
        if(len>=10) {
            size_t at=be16(p+6);
            if(at && at+2<=len) {
                size_t count=be16(p+at);
                if(count<=(len-at-2)/6)for(size_t j=0;j<count;j++)if(be32(p+at+2+j*6)==tag)found=true;
            }
        }
        CFRelease(data);if(found)return true;
    }
    if(tag==0x6b65726e) {
        CFDataRef data=CTFontCopyTable(font,kCTFontTableKern,kCTFontTableOptionNoOptions);
        if(data){CFRelease(data);return true;}
    }
    return false;
}
// Applies exactly the caller's axis values. An axis left out (optical size in
// particular) is resolved by CoreText at the logical point size.
void *figleaf_ct_instance(void *base, double size, const uint32_t *tags,
                        const double *axis_values, size_t count, const uint32_t *feature_tags,
                        const int32_t *feature_values, size_t feature_count) {
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
    if(font && feature_count) {
        CFMutableArrayRef settings=CFArrayCreateMutable(NULL,0,&kCFTypeArrayCallBacks);
        for(size_t i=0;i<feature_count;i++) {
            if(!has_feature(font,feature_tags[i])) {CFRelease(settings);CFRelease(font);return NULL;}
            uint32_t tag=feature_tags[i];uint8_t bytes[4]={tag>>24,tag>>16,tag>>8,tag};
            CFStringRef name=CFStringCreateWithBytes(NULL,bytes,4,kCFStringEncodingASCII,false);
            CFNumberRef value=CFNumberCreate(NULL,kCFNumberSInt32Type,&feature_values[i]);
            const void *keys[]={kCTFontOpenTypeFeatureTag,kCTFontOpenTypeFeatureValue};
            const void *values[]={name,value};
            CFDictionaryRef setting=CFDictionaryCreate(NULL,keys,values,2,&kCFTypeDictionaryKeyCallBacks,&kCFTypeDictionaryValueCallBacks);
            CFArrayAppendValue(settings,setting);CFRelease(setting);CFRelease(name);CFRelease(value);
        }
        const void *keys[]={kCTFontFeatureSettingsAttribute};const void *values[]={settings};
        CFDictionaryRef attrs=CFDictionaryCreate(NULL,keys,values,1,&kCFTypeDictionaryKeyCallBacks,&kCFTypeDictionaryValueCallBacks);
        CTFontDescriptorRef descriptor=CTFontDescriptorCreateWithAttributes(attrs);
        CTFontRef featured=CTFontCreateCopyWithAttributes(font,0,NULL,descriptor);
        CFRelease(font);CFRelease(descriptor);CFRelease(attrs);CFRelease(settings);font=featured;
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
        NULL, keys, values, tracking==0?2:3,
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
    uint32_t decoration, decoration_style;
    double decoration_offset, decoration_thickness;
    uint32_t custom_decoration, skip_ink;
    int32_t direction, kerning;
} StyledRun;
typedef struct { uint32_t run, kind; double points[6]; } OutlineCommand;
typedef struct { OutlineCommand *commands; size_t *count; uint32_t run; int overflow; } OutlineSink;
static void outline_element(void *info, const CGPathElement *element) {
    OutlineSink *sink=info;
    if(*sink->count>=65536){sink->overflow=1;return;}
    OutlineCommand *c=&sink->commands[(*sink->count)++];
    c->run=sink->run;c->kind=(uint32_t)element->type;
    int n=element->type==kCGPathElementAddCurveToPoint?3:
        element->type==kCGPathElementAddQuadCurveToPoint?2:
        element->type==kCGPathElementCloseSubpath?0:1;
    for(int i=0;i<n;i++){c->points[2*i]=element->points[i].x;c->points[2*i+1]=element->points[i].y;}
}
int figleaf_ct_runs(const StyledRun *input, size_t count, double scale,
                   uint8_t *pixels, uint32_t width, uint32_t rows, Metrics *metrics,
                   OutlineCommand *commands, size_t *command_count) {
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
        if(r->kerning!=0 && r->tracking==0)CFAttributedStringRemoveAttribute(attributed,range,kCTKernAttributeName);
        else CFAttributedStringSetAttribute(attributed,range,kCTKernAttributeName,kern);
        CFAttributedStringSetAttribute(attributed,range,kCTForegroundColorAttributeName,color);
        int run_index=(int)i;
        CFNumberRef run_id=CFNumberCreate(NULL,kCFNumberIntType,&run_index);
        CFAttributedStringSetAttribute(attributed,range,CFSTR("figleafRun"),run_id);
        CFRelease(run_id);
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
    metrics->ascent=ascent;metrics->descent=descent;
    metrics->baseline=(height-ascent-descent)/2+ascent;
    metrics->glyphs=(uint32_t)CTLineGetGlyphCount(line);metrics->lines=1;
    CGMutablePathRef ink=CGPathCreateMutable();
    CFArrayRef runs=CTLineGetGlyphRuns(line);
    // Tracking is removed at the physical line end, which need not belong to
    // the final logical style in a bidirectional line.
    CFIndex terminal_visual=-1;double terminal_right=-INFINITY,terminal_tracking=0;
    for(CFIndex i=0;i<CFArrayGetCount(runs);i++) {
        CTRunRef run=(CTRunRef)CFArrayGetValueAtIndex(runs,i);
        CFIndex n=CTRunGetGlyphCount(run);if(n>512){status=4;break;}
        CGPoint positions[512];CGSize advances[512];
        CTRunGetPositions(run,CFRangeMake(0,n),positions);
        CTRunGetAdvances(run,CFRangeMake(0,n),advances);
        for(CFIndex j=0;j<n;j++)if(positions[j].x+advances[j].width>terminal_right) {
            int index=0;
            CFNumberGetValue(CFDictionaryGetValue(CTRunGetAttributes(run),CFSTR("figleafRun")),kCFNumberIntType,&index);
            terminal_right=positions[j].x+advances[j].width;
            terminal_tracking=input[index].tracking;terminal_visual=i;
        }
    }
    if(terminal_visual>=0)metrics->advance-=terminal_tracking;
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
        if(!status && commands) {
            int run_index=0;
            CFNumberGetValue(CFDictionaryGetValue(CTRunGetAttributes(run),CFSTR("figleafRun")),kCFNumberIntType,&run_index);
            OutlineSink sink={commands,command_count,(uint32_t)run_index,0};
            CGPoint positions[512];CTRunGetPositions(run,CFRangeMake(0,n),positions);
            for(CFIndex j=0;j<n;j++) {
                CGAffineTransform transform=CGAffineTransformMake(1,0,0,-1,positions[j].x,metrics->baseline-positions[j].y);
                CGPathRef path=CTFontCreatePathForGlyph(actual,glyphs[j],&transform);
                if(path){CGPathAddPath(ink,NULL,path);CGPathApply(path,&sink,outline_element);CGPathRelease(path);}
                else {
                    CGRect bounds;CTFontGetBoundingRectsForGlyphs(actual,kCTFontOrientationDefault,&glyphs[j],&bounds,1);
                    if(!CGRectIsEmpty(bounds))status=7;
                }
            }
            if(sink.overflow)status=4;
        }
    }
    if(!status && (pixels || commands)) {
        if(pixels)memset(pixels,0,(size_t)width*rows*4);
        CGContextRef ctx=pixels?CGBitmapContextCreate(pixels,width,rows,8,width*4,space,kCGImageAlphaPremultipliedLast|kCGBitmapByteOrder32Big):NULL;
        if(pixels && !ctx)status=5;else {
            if(ctx) {
                CGContextSetAllowsFontSmoothing(ctx,false);CGContextSetShouldSmoothFonts(ctx,false);
                CGContextSetShouldAntialias(ctx,true);CGContextSetShouldSubpixelPositionFonts(ctx,true);
                CGContextSetShouldSubpixelQuantizeFonts(ctx,false);CGContextScaleCTM(ctx,scale,scale);
            }
            double box_height=pixels?(double)rows/scale:height;
            double baseline=box_height-metrics->baseline;
            if(ctx){CGContextSetTextPosition(ctx,0,baseline);CTLineDraw(line,ctx);}
            // A logical style can occupy disjoint visual spans after bidi ordering.
            // Decorate each shaped run, never the two logical range endpoints.
            for(CFIndex visual=0;visual<CFArrayGetCount(runs);visual++) {
                CTRunRef run=(CTRunRef)CFArrayGetValueAtIndex(runs,visual);
                int i=0;
                CFNumberGetValue(CFDictionaryGetValue(CTRunGetAttributes(run),CFSTR("figleafRun")),kCFNumberIntType,&i);
                const StyledRun *r=&input[i];
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
                    CFIndex n=CTRunGetGlyphCount(run);
                    if(n==0)continue;
                    CGPoint positions[512];CGSize advances[512];
                    CTRunGetPositions(run,CFRangeMake(0,n),positions);
                    CTRunGetAdvances(run,CFRangeMake(0,n),advances);
                    double x=positions[0].x,end_x=x;
                    for(CFIndex j=0;j<n;j++) {
                        x=fmin(x,positions[j].x);
                        end_x=fmax(end_x,positions[j].x+advances[j].width);
                    }
                    if(visual==terminal_visual)end_x=metrics->advance;
                    if(ctx)CGContextSetRGBFillColor(ctx,r->color[0],r->color[1],r->color[2],r->color[3]);
                    // Figma places the strike at the font's strikeout position,
                    // and the underline below the font's underline position.
                    double bottom=r->decoration==1
                        ? baseline+CTFontGetUnderlinePosition(font)-thickness*1.5
                        : baseline+position-thickness;
                    if(!r->custom_decoration && (bottom<0 || bottom+thickness>box_height || thickness<=0)) {
                        status=6;break;
                    }
                    if(r->custom_decoration) {
                        if(!commands){status=7;break;}
                        double top=isnan(r->decoration_offset)
                            ? metrics->baseline-CTFontGetUnderlinePosition(font)+thickness*.5
                            : metrics->baseline+r->decoration_offset;
                        if(!isnan(r->decoration_thickness))thickness=r->decoration_thickness;
                        if(thickness==0)continue;
                        if((end_x-x)/thickness>8192){status=4;break;}
                        CGMutablePathRef decoration=CGPathCreateMutable();
                        if(r->decoration_style==0)CGPathAddRect(decoration,NULL,CGRectMake(x,top,end_x-x,thickness));
                        else if(r->decoration_style==2) {
                            for(double left=x;left+thickness<=end_x;left+=2*thickness)
                                CGPathAddEllipseInRect(decoration,NULL,CGRectMake(left,top,thickness,thickness));
                        } else {
                            CGMutablePathRef wave=CGPathCreateMutable();
                            double half=1.75*thickness,y=top+thickness;
                            CGPathMoveToPoint(wave,NULL,x,y);
                            for(double left=x;left+half<=end_x;left+=half) {
                                double next=y==top?top+thickness:top;
                                CGPathAddCurveToPoint(wave,NULL,left+half/2,y,left+half/2,next,left+half,next);y=next;
                            }
                            CGPathRef stroke=CGPathCreateCopyByStrokingPath(wave,NULL,thickness*2/3,kCGLineCapRound,kCGLineJoinRound,10);
                            CGPathAddPath(decoration,NULL,stroke);CGPathRelease(stroke);CGPathRelease(wave);
                        }
                        CGPathRef result=CGPathCreateCopy(decoration);CGPathRelease(decoration);
                        if(r->skip_ink) {
                            if(__builtin_available(macOS 13.0, *)) {
                                // Only ink that actually intersects the decoration opens a gap.
                                CGRect band=CGPathGetPathBoundingBox(result);
                                CGPathRef intersections=CGPathCreateCopyByIntersectingPath(ink,result,false);
                                CFArrayRef parts=intersections?CGPathCreateSeparateComponents(intersections,false):NULL;
                                CGMutablePathRef gaps=CGPathCreateMutable();
                                if(!parts){status=7;} else for(CFIndex k=0;k<CFArrayGetCount(parts);k++) {
                                    CGRect b=CGPathGetPathBoundingBox((CGPathRef)CFArrayGetValueAtIndex(parts,k));
                                    CGPathAddRect(gaps,NULL,CGRectMake(b.origin.x-thickness,band.origin.y-thickness,b.size.width+2*thickness,band.size.height+2*thickness));
                                }
                                CGPathRef cut=CGPathCreateCopyBySubtractingPath(result,gaps,false);
                                CGPathRelease(result);result=cut;
                                CGPathRelease(gaps);if(parts)CFRelease(parts);if(intersections)CGPathRelease(intersections);
                            } else status=7;
                        }
                        if(!result)status=7;
                        else {
                            OutlineSink sink={commands,command_count,(uint32_t)(count+i),0};
                            CGPathApply(result,&sink,outline_element);CGPathRelease(result);
                            if(sink.overflow)status=4;
                        }
                    } else {
                        if(ctx)CGContextFillRect(ctx,CGRectMake(x,bottom,end_x-x,thickness));
                        if(commands) {
                            OutlineSink sink={commands,command_count,(uint32_t)(count+i),0};
                            CGPathRef path=CGPathCreateWithRect(CGRectMake(x,box_height-bottom-thickness,end_x-x,thickness),NULL);
                            CGPathApply(path,&sink,outline_element);CGPathRelease(path);
                            if(sink.overflow)status=4;
                        }
                    }
                }
            }
            if(ctx)CGContextRelease(ctx);
        }
    }
    CGPathRelease(ink);CFRelease(line);CFRelease(attributed);CGColorSpaceRelease(space);return status;
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
    CFStringRef string=CFStringCreateWithBytes(NULL,text,len,kCFStringEncodingUTF8,false);
    if(!string)return false;
    CFMutableAttributedStringRef attr=CFAttributedStringCreateMutable(NULL,0);
    CFAttributedStringReplaceString(attr,CFRangeMake(0,0),string);
    CFAttributedStringSetAttribute(attr,CFRangeMake(0,CFStringGetLength(string)),kCTFontAttributeName,font);
    CTLineRef line=CTLineCreateWithAttributedString(attr);
    CFStringRef expected=CTFontCopyPostScriptName(font);
    bool covered=true;CFArrayRef runs=CTLineGetGlyphRuns(line);
    for(CFIndex i=0;i<CFArrayGetCount(runs);i++) {
        CTRunRef run=(CTRunRef)CFArrayGetValueAtIndex(runs,i);
        CTFontRef actual=(CTFontRef)CFDictionaryGetValue(CTRunGetAttributes(run),kCTFontAttributeName);
        CFStringRef name=CTFontCopyPostScriptName(actual);
        if(!CFEqual(name,expected))covered=false;CFRelease(name);
        CFIndex count=CTRunGetGlyphCount(run);if(count>512){covered=false;break;}
        CGGlyph glyphs[512];CTRunGetGlyphs(run,CFRangeMake(0,count),glyphs);
        for(CFIndex j=0;j<count;j++)if(!glyphs[j])covered=false;
    }
    CFRelease(expected);CFRelease(line);CFRelease(attr);CFRelease(string);return covered;
}
int figleaf_ct_direction(const uint8_t *text,size_t len) {
    CFStringRef string=CFStringCreateWithBytes(NULL,text,len,kCFStringEncodingUTF8,false);
    if(!string)return 0;
    CFCharacterSetRef letters=CFCharacterSetGetPredefined(kCFCharacterSetLetter);
    CFCharacterSetRef marks=CFCharacterSetGetPredefined(kCFCharacterSetNonBase);
    int direction=0;
    // First strong scalar, including supplementary historic and RTL scripts.
    for(CFIndex i=0;i<CFStringGetLength(string);i++) {
        UTF32Char c=CFStringGetCharacterAtIndex(string,i);
        if(CFStringIsSurrogateHighCharacter((UniChar)c) && i+1<CFStringGetLength(string)) {
            UniChar low=CFStringGetCharacterAtIndex(string,++i);c=CFStringGetLongCharacterForSurrogatePair((UniChar)c,low);
        }
        if(CFCharacterSetIsLongCharacterMember(letters,c) && !CFCharacterSetIsLongCharacterMember(marks,c)) {
            direction=(c>=0x0590 && c<=0x08ff) || (c>=0xfb1d && c<=0xfdff) || (c>=0xfe70 && c<=0xfefc) || (c>=0x10800 && c<=0x10fff) || (c>=0x1e800 && c<=0x1eeff);break;
        }
    }
    CFRelease(string);return direction;
}
