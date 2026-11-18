// SPDX-License-Identifier: GPL-2.0-only
#include "olgp_abi.h"

#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

int main(int argc, char **argv)
{
	int fd;
	struct olgp_submit submit;
	struct olgp_response response;
	const char *path = argc > 1 ? argv[1] : "/dev/dri/card0";

	fd = open(path, O_RDWR);
	if (fd < 0) {
		perror("open");
		return 1;
	}

	memset(&submit, 0, sizeof(submit));
	submit.opcode = 1;
	submit.request_id = 1;
	if (ioctl(fd, OLGP_IOC_SUBMIT, &submit) < 0) {
		perror("submit");
		close(fd);
		return 1;
	}

	memset(&response, 0, sizeof(response));
	if (ioctl(fd, OLGP_IOC_RECV, &response) < 0) {
		perror("recv");
		close(fd);
		return 1;
	}

	printf("request=%u status=%u\n", response.request_id, response.status);
	close(fd);
	return 0;
}
