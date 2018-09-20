#include <stdio.h>

int calc (int a, int b) {
	int d = b * 2;
	int c = a + d;
	return c;
}

int add (int a, int b) {
	return a + b;
}

void main () {
	int r = calc (10, 20);
	printf("0x%02x\n", r);
}

