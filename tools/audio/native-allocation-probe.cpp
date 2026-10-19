#include <cstdlib>
#include <new>
#include <cstdio>
#include <cmath>
#include <cstddef>
static bool guarded=false;
static unsigned long allocs=0, frees=0;
void *operator new(std::size_t n) { if(guarded)++allocs; if(auto p=std::malloc(n))return p;throw std::bad_alloc(); }
void *operator new[](std::size_t n) {return ::operator new(n);}
void operator delete(void *p) noexcept {if(guarded&&p)++frees;std::free(p);}
void operator delete[](void *p) noexcept {::operator delete(p);}
#include "signalsmith-stretch.h"
int main() {
 signalsmith::stretch::SignalsmithStretch<float> p(0);p.configure(2,3072,768,true);
 float left[8192],right[8192],a[64],b[64];const float *in[]={left,right};float*out[]={a,b};
 for(int i=0;i<8192;++i){left[i]=std::sin(i*.23)+std::sin(i*.91)*.2;right[i]=-left[i];}
 guarded=true;
 for(int i=0;i<2000;++i){float rate=.1+1.97*(i%100)/100.f;p.setTransposeFactor(std::pow(2.f,(i%24-12)/12.f));if(i%97==0)p.outputSeek(in,p.outputSeekLength(rate));p.process(in,int(rate*64),out,64);}
 guarded=false;
 std::printf("{\"native_allocations\":%lu,\"native_deallocations\":%lu,\"process_calls\":2000,\"seek_calls\":21}\n",allocs,frees);
 return allocs||frees?1:0;
}
