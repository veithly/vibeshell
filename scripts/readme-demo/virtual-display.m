#import <Foundation/Foundation.h>
#import <CoreGraphics/CoreGraphics.h>
#import <dispatch/dispatch.h>
#import <objc/runtime.h>
#import <signal.h>

// Temporary, unbundled capture helper. Private classes are loaded dynamically.
@interface CGVirtualDisplayDescriptor : NSObject
@property(nonatomic, strong) NSString* name;
@property(nonatomic, strong) dispatch_queue_t queue;
@property(nonatomic) CGPoint whitePoint, redPrimary, greenPrimary, bluePrimary;
@property(nonatomic) CGSize sizeInMillimeters;
@property(nonatomic) uint32_t maxPixelsWide, maxPixelsHigh;
@property(nonatomic) uint32_t vendorID, productID, serialNum, serialNumber;
@end
@interface CGVirtualDisplayMode : NSObject
-(instancetype)initWithWidth:(uint32_t)width height:(uint32_t)height refreshRate:(double)rate;
@end
@interface CGVirtualDisplaySettings : NSObject
@property(nonatomic, strong) NSArray* modes;
@property(nonatomic) BOOL hiDPI;
@property(nonatomic) uint32_t rotation;
@end
@interface CGVirtualDisplay : NSObject
-(instancetype)initWithDescriptor:(CGVirtualDisplayDescriptor*)descriptor;
-(BOOL)applySettings:(CGVirtualDisplaySettings*)settings;
@property(nonatomic, readonly) uint32_t displayID;
@end
static volatile sig_atomic_t stop = 0;
static void handler(int signum) { (void)signum; stop=1; }
int main() {
 @autoreleasepool {
  if (!NSClassFromString(@"CGVirtualDisplayDescriptor") || !NSClassFromString(@"CGVirtualDisplay")) { NSLog(@"Virtual display unsupported");return 2; }
  CGVirtualDisplayDescriptor* desc = [[NSClassFromString(@"CGVirtualDisplayDescriptor") alloc] init];
  desc.name=@"VibeShell README Capture";
  desc.queue=dispatch_get_main_queue();
  desc.whitePoint=CGPointMake(.3125,.3291);
  desc.redPrimary=CGPointMake(.6797,.3203);
  desc.greenPrimary=CGPointMake(.2559,.6983);
  desc.bluePrimary=CGPointMake(.1494,.0557);
  desc.maxPixelsWide=1440;desc.maxPixelsHigh=900;
  desc.sizeInMillimeters=CGSizeMake(406.4,254);
  desc.vendorID=505; desc.productID=400; desc.serialNum=26501; desc.serialNumber=26501;
  CGVirtualDisplay* display=[[NSClassFromString(@"CGVirtualDisplay") alloc] initWithDescriptor:desc];
  if (!display) { NSLog(@"Failed creating virtual display");return 3; }
  CGVirtualDisplaySettings* settings=[[NSClassFromString(@"CGVirtualDisplaySettings") alloc] init];
  settings.hiDPI=NO;settings.rotation=0;
  CGVirtualDisplayMode* mode=[[NSClassFromString(@"CGVirtualDisplayMode") alloc] initWithWidth:1440 height:900 refreshRate:60];
  settings.modes=@[mode];
  if (![display applySettings:settings]) { NSLog(@"Failed applying mode");return 4; }
  uint32_t displayID=display.displayID;
  CGRect bounds=CGDisplayBounds(displayID);
  printf("VIRTUAL_DISPLAY_READY id=%u bounds=%.0f,%.0f,%.0f,%.0f\n", displayID,bounds.origin.x,bounds.origin.y,bounds.size.width,bounds.size.height);fflush(stdout);
  signal(SIGTERM,handler);signal(SIGINT,handler);
  while (!stop) { @autoreleasepool { [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:.15]]; } }
  printf("VIRTUAL_DISPLAY_REMOVED id=%u\n",displayID);fflush(stdout);
  display=nil;
 }
 return 0;
}
